// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Queue of encoded OTLP batches between the [`Exporter`](crate::Exporter) and the transport.
///
/// The exporter writes every batch here *before* trying the network (write-ahead), uploads
/// oldest-first, and removes what the collector accepted or rejected. Two implementations ship:
/// [`MemoryCache`] (default) and [`FileCache`] (survives crashes and restarts); anything else —
/// a database, a platform store — plugs in through
/// [`Telemetry::with_cache`](crate::Telemetry::with_cache).
///
/// Ids are chosen by the exporter as `<unix_ns>-<seq>-<event_count>`: sortable, so
/// [`pending`](Self::pending) is a plain sort, and prefixed with the creation time so an
/// implementation can expire old batches without touching file timestamps. Implementations
/// bound their own footprint by evicting the oldest batches; the exporter never sees eviction.
pub trait BatchCache: Send + Sync {
    /// Store one encoded batch under `id`.
    fn push(&self, id: &str, body: &[u8]) -> io::Result<()>;
    /// Ids of stored batches, oldest first.
    fn pending(&self) -> Vec<String>;
    /// The body stored under `id`, if it is still there.
    fn read(&self, id: &str) -> Option<Vec<u8>>;
    fn remove(&self, id: &str);
    /// Discard everything (telemetry disabled: nothing may be replayed later).
    fn clear(&self);
}

/// In-memory [`BatchCache`]: batches that could not be uploaded wait for the next attempt,
/// bounded by `max_bytes` (oldest evicted). Lost with the process.
// ponytail: a Vec with remove(0) — a handful of small batches, and it shares Vec code the
// binary already has instead of pulling in VecDeque's ring-buffer instantiations.
pub struct MemoryCache {
    batches: Mutex<Vec<(String, Vec<u8>)>>,
    max_bytes: usize,
}

impl MemoryCache {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
            max_bytes: usize::try_from(max_bytes).unwrap_or(usize::MAX),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, Vec<u8>)>> {
        self.batches.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl BatchCache for MemoryCache {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<()> {
        let mut batches = self.lock();
        batches.push((id.to_owned(), body.to_vec()));
        let mut total: usize = batches.iter().map(|(_, b)| b.len()).sum();
        while total > self.max_bytes && batches.len() > 1 {
            total -= batches.remove(0).1.len();
        }
        Ok(())
    }

    fn pending(&self) -> Vec<String> {
        self.lock().iter().map(|(id, _)| id.clone()).collect()
    }

    fn read(&self, id: &str) -> Option<Vec<u8>> {
        self.lock().iter().find(|(i, _)| i == id).map(|(_, body)| body.clone())
    }

    fn remove(&self, id: &str) {
        let mut batches = self.lock();
        if let Some(index) = batches.iter().position(|(i, _)| i == id) {
            batches.remove(index);
        }
    }

    fn clear(&self) {
        self.lock().clear();
    }
}

const EXT: &str = "otlp";
// ponytail: fixed 24 h max age (Datadog: 18 h, design doc: 24 h); a config knob if anyone asks.
const MAX_AGE_NANOS: u64 = 24 * 60 * 60 * 1_000_000_000;

/// On-disk [`BatchCache`]: one file per encoded batch, so a crash or an offline shutdown loses
/// nothing and the next launch replays what is left.
///
/// The shape every mobile SDK converges on (Sentry envelopes, Datadog batch files,
/// opentelemetry-android disk buffering): files are written as `.tmp` and renamed into place so
/// a crash never leaves a half batch readable; eviction is drop-oldest above `max_bytes`;
/// batches older than 24 h are discarded, judged by the timestamp in the id rather than file
/// metadata (an Apple "required reason" API).
pub struct FileCache {
    dir: PathBuf,
    max_bytes: u64,
}

impl FileCache {
    /// Open the cache directory (created if missing; its parent must exist) and discard stale
    /// or half-written files.
    pub fn open(dir: impl Into<PathBuf>, max_bytes: u64) -> io::Result<Self> {
        let dir = dir.into();
        // ponytail: `create_dir`, not `create_dir_all` — the recursive variant drags in
        // `Path::components` machinery (~3 KiB) for a parent the host always provides.
        match fs::create_dir(&dir) {
            Err(err) if err.kind() != io::ErrorKind::AlreadyExists => return Err(err),
            _ => {}
        }
        let cache = Self { dir, max_bytes };
        cache.prune()?;
        Ok(cache)
    }

    fn path(&self, id: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{id}.{ext}"))
    }

    fn write_then_rename(&self, tmp: &Path, dest: &Path, body: &[u8]) -> io::Result<()> {
        if let Err(err) = fs::write(tmp, body) {
            // iOS may purge the whole Caches subdirectory while the app runs: recreate it once.
            if err.kind() != io::ErrorKind::NotFound {
                return Err(err);
            }
            fs::create_dir(&self.dir)?;
            fs::write(tmp, body)?;
        }
        fs::rename(tmp, dest)
    }

    /// Delete stray `.tmp` files and batches older than the max age, then the oldest batches
    /// until the total fits `max_bytes`.
    fn prune(&self) -> io::Result<()> {
        let now = crate::event::now_unix_nanos();
        for entry in fs::read_dir(&self.dir)?.flatten() {
            let path = entry.path();
            let expired = stamp(&path).is_some_and(|t| now.saturating_sub(t) > MAX_AGE_NANOS);
            if !is_batch(&path) || expired {
                let _ = fs::remove_file(&path);
            }
        }
        let kept = self.pending();
        let sizes: Vec<u64> = kept
            .iter()
            .map(|id| fs::metadata(self.path(id, EXT)).map(|m| m.len()).unwrap_or(0))
            .collect();
        let mut total: u64 = sizes.iter().sum();
        for (id, len) in kept.iter().zip(sizes) {
            if total <= self.max_bytes {
                break;
            }
            self.remove(id);
            total -= len;
        }
        Ok(())
    }
}

impl BatchCache for FileCache {
    /// Fail-open: a full disk or a purged directory yields an error the exporter counts as a
    /// drop; it never leaves a partial `.tmp` behind.
    fn push(&self, id: &str, body: &[u8]) -> io::Result<()> {
        let tmp = self.path(id, "tmp");
        let written = self.write_then_rename(&tmp, &self.path(id, EXT), body);
        if written.is_err() {
            // ENOSPC leaves a truncated `.tmp`; drop it now rather than at the next launch.
            let _ = fs::remove_file(&tmp);
        }
        written?;
        self.prune()
    }

    fn pending(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Vec::new() };
        let mut ids: Vec<String> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_batch(p))
            .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
            .collect();
        ids.sort_unstable();
        ids
    }

    fn read(&self, id: &str) -> Option<Vec<u8>> {
        fs::read(self.path(id, EXT)).ok()
    }

    fn remove(&self, id: &str) {
        let _ = fs::remove_file(self.path(id, EXT));
    }

    fn clear(&self) {
        for id in self.pending() {
            self.remove(&id);
        }
    }
}

fn is_batch(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXT)
}

/// The creation time encoded in a batch file name.
fn stamp(path: &Path) -> Option<u64> {
    path.file_stem()?.to_str()?.split('-').next()?.parse().ok()
}

#[cfg(test)]
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("livekit-telemetry-{tag}-{}", crate::event::now_unix_nanos()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids the exporter would mint: a real timestamp (so `prune` does not expire them) + seq.
    fn id(n: u64) -> String {
        format!(
            "{:020}-{n:06}-1",
            crate::event::now_unix_nanos() / 1_000_000_000 * 1_000_000_000 + n
        )
    }

    #[test]
    fn memory_cache_evicts_oldest_beyond_max_bytes() {
        let cache = MemoryCache::new(25);
        for (n, body) in [b"aaaaaaaaaa", b"bbbbbbbbbb", b"cccccccccc"].into_iter().enumerate() {
            cache.push(&id(n as u64), body).expect("push");
        }
        assert_eq!(cache.pending(), [id(1), id(2)]);
        assert_eq!(cache.read(&id(1)).expect("read"), b"bbbbbbbbbb");
        cache.remove(&id(1));
        assert_eq!(cache.pending(), [id(2)]);
        cache.clear();
        assert!(cache.pending().is_empty());
    }

    #[test]
    fn file_cache_evicts_oldest_beyond_max_bytes_and_drops_stray_tmp() {
        let dir = temp_dir("cache");
        let cache = FileCache::open(&dir, 25).expect("open");
        fs::write(dir.join("crashed.tmp"), b"half").expect("write");
        for (n, body) in [b"aaaaaaaaaa", b"bbbbbbbbbb", b"cccccccccc"].into_iter().enumerate() {
            cache.push(&id(n as u64), body).expect("push");
        }
        assert_eq!(cache.pending(), [id(1), id(2)], "10-byte batches under a 25-byte cap");
        assert_eq!(cache.read(&id(1)).expect("read"), b"bbbbbbbbbb");
        assert!(!dir.join("crashed.tmp").exists());
        cache.clear();
        assert!(cache.pending().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_cache_recreates_a_purged_directory() {
        let dir = temp_dir("purged");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        fs::remove_dir_all(&dir).expect("purge like iOS does");
        cache.push(&id(1), b"after purge").expect("push recreates the dir");
        assert_eq!(cache.pending(), [id(1)]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn file_cache_failed_write_leaves_no_partial_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("readonly");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        // Stand-in for ENOSPC: any write into the directory fails.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = cache.push(&id(1), b"no room");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod back");
        assert!(result.is_err());
        assert_eq!(fs::read_dir(&dir).expect("dir").count(), 0, "no stray .tmp");
        let _ = fs::remove_dir_all(&dir);
    }
}
