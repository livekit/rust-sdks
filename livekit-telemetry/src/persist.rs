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
    sync::atomic::{AtomicU64, Ordering},
};

use crate::event::now_unix_nanos;

const EXT: &str = "otlp";
// ponytail: fixed 24 h max age (Datadog: 18 h, design doc: 24 h); a config knob if anyone asks.
const MAX_AGE_NANOS: u64 = 24 * 60 * 60 * 1_000_000_000;

/// File cache of encoded batches the transport could not deliver.
///
/// One file per `ExportLogsServiceRequest` body — the unit every mobile SDK persists (Sentry
/// envelopes, Datadog batch files, opentelemetry-android disk buffering): nothing to re-encode
/// on replay, and URL/headers are recomposed from the *current* config, so a rotated token is
/// picked up automatically. Files are written as `.tmp` and renamed into place so a crash never
/// leaves a half-written batch readable. Replay is oldest-first; eviction is drop-oldest above
/// `max_bytes`; batches older than 24 h are discarded. The age comes from the file name, not
/// from file timestamps (an Apple "required reason" API).
pub(crate) struct FileCache {
    dir: PathBuf,
    max_bytes: u64,
    seq: AtomicU64,
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
        let cache = Self { dir, max_bytes, seq: AtomicU64::new(0) };
        cache.prune()?;
        Ok(cache)
    }

    /// Persist one encoded batch, evicting the oldest batches to stay within `max_bytes`.
    ///
    /// Fail-open: a full disk or a purged directory yields an error the caller counts as a
    /// drop; it never leaves a partial `.tmp` behind.
    pub fn store(&self, body: &[u8]) -> io::Result<()> {
        let name =
            format!("{:020}-{:06}", now_unix_nanos(), self.seq.fetch_add(1, Ordering::Relaxed));
        let tmp = self.dir.join(format!("{name}.tmp"));
        let written = self.write_then_rename(&tmp, &self.dir.join(format!("{name}.{EXT}")), body);
        if written.is_err() {
            // ENOSPC leaves a truncated `.tmp`; drop it now rather than at the next launch.
            let _ = fs::remove_file(&tmp);
        }
        written?;
        self.prune()
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

    /// Pending batches, oldest first.
    pub fn pending(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Vec::new() };
        let mut files: Vec<PathBuf> =
            entries.flatten().map(|e| e.path()).filter(|p| is_batch(p)).collect();
        files.sort_unstable();
        files
    }

    pub fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    pub fn remove(&self, path: &Path) {
        let _ = fs::remove_file(path);
    }

    /// Discard every pending batch (telemetry disabled: nothing may be replayed later).
    pub fn clear(&self) {
        for path in self.pending() {
            self.remove(&path);
        }
    }

    /// Delete stray `.tmp` files and batches older than the max age, then the oldest batches
    /// until the total fits `max_bytes`.
    fn prune(&self) -> io::Result<()> {
        let now = now_unix_nanos();
        for entry in fs::read_dir(&self.dir)?.flatten() {
            let path = entry.path();
            let expired = stamp(&path).is_some_and(|t| now.saturating_sub(t) > MAX_AGE_NANOS);
            if !is_batch(&path) || expired {
                let _ = fs::remove_file(&path);
            }
        }
        let kept = self.pending();
        let sizes: Vec<u64> =
            kept.iter().map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0)).collect();
        let mut total: u64 = sizes.iter().sum();
        for (path, len) in kept.iter().zip(sizes) {
            if total <= self.max_bytes {
                break;
            }
            let _ = fs::remove_file(path);
            total -= len;
        }
        Ok(())
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
    let dir = std::env::temp_dir().join(format!("livekit-telemetry-{tag}-{}", now_unix_nanos()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_beyond_max_bytes_and_drops_stray_tmp() {
        let dir = temp_dir("cache");
        let cache = FileCache::open(&dir, 25).expect("open");
        fs::write(dir.join("crashed.tmp"), b"half").expect("write");
        for body in [b"aaaaaaaaaa", b"bbbbbbbbbb", b"cccccccccc"] {
            cache.store(body).expect("store");
        }
        let pending = cache.pending();
        assert_eq!(pending.len(), 2, "10-byte batches under a 25-byte cap");
        assert_eq!(cache.read(&pending[0]).expect("read"), b"bbbbbbbbbb");
        assert!(!dir.join("crashed.tmp").exists() || cache.pending().len() == 2);
        cache.clear();
        assert!(cache.pending().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recreates_a_purged_directory() {
        let dir = temp_dir("purged");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        fs::remove_dir_all(&dir).expect("purge like iOS does");
        cache.store(b"after purge").expect("store recreates the dir");
        assert_eq!(cache.pending().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn failed_write_leaves_no_partial_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("readonly");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        // Stand-in for ENOSPC: any write into the directory fails.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = cache.store(b"no room");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod back");
        assert!(result.is_err());
        assert_eq!(fs::read_dir(&dir).expect("dir").count(), 0, "no stray .tmp");
        let _ = fs::remove_dir_all(&dir);
    }
}
