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
    sync::{atomic::Ordering, Mutex},
};

/// Queue of encoded OTLP batches between the [`Exporter`](crate::Exporter) and the transport.
///
/// The exporter writes every batch here *before* trying the network (write-ahead), uploads
/// oldest-first, and removes what the collector accepted or rejected. Two implementations ship:
/// [`MemoryCache`] (default) and [`FileCache`] (survives crashes and restarts); anything else —
/// a database, a platform store — plugs in through
/// [`Telemetry::with_cache`](crate::Telemetry::with_cache).
///
/// Ids are chosen by the exporter as `<unix_ns>-<seq>-<event_count>-<signal>[-<host>]`, made of
/// filename-safe characters: sortable, so [`pending`](Self::pending) is a plain sort, and
/// prefixed with the creation time so an implementation can expire old batches without touching
/// file timestamps. Implementations bound their own footprint by evicting the oldest batches and
/// report what they evicted, so every loss is counted. Storage failures are reported, never
/// swallowed: a delete that did not happen must not look like one that did.
pub trait BatchCache: Send + Sync {
    /// Store one encoded batch under `id`: once this returns `Ok` the batch is committed (for
    /// [`FileCache`], on disk). Returns the ids of older batches evicted to make room (the
    /// exporter counts their events as dropped).
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>>;
    /// Replace the batch `old` with `new` (a split): all of `new` is committed before `old`
    /// goes, and nothing is evicted in between, so a crash can duplicate records but never lose
    /// them. On `Err`, `old` is still there. Returns the ids evicted afterwards.
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>>;
    /// Ids of stored batches, oldest first.
    fn pending(&self) -> Vec<String>;
    /// The body stored under `id`: `ErrorKind::NotFound` when it is gone, another error when it
    /// exists but cannot be read right now (e.g. a locked device's file protection).
    fn read(&self, id: &str) -> io::Result<Vec<u8>>;
    /// Delete one batch; a batch already gone is not an error.
    fn remove(&self, id: &str) -> io::Result<()>;
    /// Discard everything (telemetry disabled: nothing may be replayed later); `Err` when
    /// something could not be deleted.
    fn clear(&self) -> io::Result<()>;
    /// Ids evicted before the exporter could see them (over the bounds when the cache
    /// was opened), handed over once so they are counted as lost.
    fn take_evicted(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Batches a cache will hold regardless of their size. `max_cache_bytes` is the size policy;
/// this only stops a long offline stretch at a 1 s cadence from leaving thousands of tiny files
/// in the directory.
// ponytail: a flat cap rather than another config knob; make it one if a host ever needs
// buffering at a scale where the file count matters.
pub(crate) const MAX_BATCHES: usize = 512;

/// In-memory [`BatchCache`]: batches that could not be uploaded wait for the next attempt,
/// bounded by `max_bytes` and [`MAX_BATCHES`] (oldest evicted). Lost with the process.
// ponytail: a Vec with remove(0) — a handful of small batches, and it shares Vec code the
// binary already has instead of pulling in VecDeque's ring-buffer instantiations.
pub struct MemoryCache {
    batches: Mutex<Vec<(String, Vec<u8>)>>,
    max_bytes: usize,
}

impl MemoryCache {
    /// An empty cache holding at most `max_bytes` of batches.
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
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        let mut batches = self.lock();
        batches.push((id.to_owned(), body.to_vec()));
        batches.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(self.evict(&mut batches))
    }

    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        let mut batches = self.lock();
        batches.retain(|(id, _)| id != old);
        batches.extend(new.iter().cloned());
        batches.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(self.evict(&mut batches))
    }

    fn pending(&self) -> Vec<String> {
        self.lock().iter().map(|(id, _)| id.clone()).collect()
    }

    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        self.lock()
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, body)| body.clone())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn remove(&self, id: &str) -> io::Result<()> {
        self.lock().retain(|(i, _)| i != id);
        Ok(())
    }

    fn clear(&self) -> io::Result<()> {
        self.lock().clear();
        Ok(())
    }
}

impl MemoryCache {
    /// Drop the oldest batches until the rest fit; returns their ids.
    fn evict(&self, batches: &mut Vec<(String, Vec<u8>)>) -> Vec<String> {
        let mut total: usize = batches.iter().map(|(_, b)| b.len()).sum();
        let mut evicted = Vec::new();
        while (total > self.max_bytes || batches.len() > MAX_BATCHES) && batches.len() > 1 {
            let (id, body) = batches.remove(0);
            total -= body.len();
            evicted.push(id);
        }
        evicted
    }
}

const EXT: &str = "otlp";

/// On-disk [`BatchCache`]: one file per encoded batch, so a crash or an offline shutdown loses
/// nothing and the next launch replays what is left.
///
/// Files are written as `.tmp` and renamed into place, so a crash never leaves a half batch
/// readable; eviction is drop-oldest above `max_bytes` or [`MAX_BATCHES`] files. Age is the
/// exporter's call (24 h, judged by the timestamp in the id rather than file metadata — an
/// Apple "required reason" API).
pub struct FileCache {
    dir: PathBuf,
    max_bytes: u64,
    /// File-count bound: [`MAX_BATCHES`] (lowered only by tests).
    max_batches: usize,
    /// What opening pruned, until [`BatchCache::take_evicted`] collects it.
    evicted: Mutex<Vec<String>>,
    /// Publishing a journaled split failed: listings stop retrying (and logging) until the cache
    /// writes again.
    recovery_failed: std::sync::atomic::AtomicBool,
    /// `recover` runs so far (tests only).
    #[cfg(test)]
    recoveries: std::sync::atomic::AtomicUsize,
    /// Test-only fault points around every storage step (see [`Step`]).
    #[cfg(test)]
    fault: Mutex<Option<Fault>>,
}

/// A storage step: the points a test can fail (return an error) or crash at (panic),
/// deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// Creating and writing a `.tmp` (the path is passed).
    Write,
    /// `fsync` of a `.tmp`.
    Sync,
    /// Renaming a `.tmp` into place, or into a replacement's journal (`.pend`).
    Rename,
    /// `fsync` of the directory.
    SyncDir,
    /// Deleting a batch (including a replaced one — a split's commit point).
    Delete,
    /// Publishing a committed replacement half (`.pend` → batch).
    Publish,
}

#[cfg(test)]
type Fault = Box<dyn Fn(Step, &Path) -> io::Result<()> + Send + Sync>;

const PEND: &str = "pend";
/// Separates parent and half in a journal file name. Batch ids never contain it (ids are digits,
/// hex, `l`/`t`, `-` and a URL-parsed host, which cannot hold `@`).
const JOURNAL: char = '@';

/// Held across a split's journaled section and across recovery, by every [`FileCache`] in the
/// process: a reconfigure opens a second cache on the directory while the replaced pipeline still
/// drains, and recovery must never see that pipeline's split half-done — it would delete the
/// journal of a split about to commit, losing the halves.
// ponytail: process-wide rather than per directory; splits are rare and short (a few fsyncs).
static SPLITS: Mutex<()> = Mutex::new(());

fn splits() -> std::sync::MutexGuard<'static, ()> {
    SPLITS.lock().unwrap_or_else(|e| e.into_inner())
}

impl FileCache {
    /// Open the cache directory (created if missing; its parent must exist), finish or roll back
    /// a split interrupted by a crash — before anything is evicted — and discard stray or
    /// half-written files (only here: a replaced pipeline may still be writing its own).
    pub fn open(dir: impl Into<PathBuf>, max_bytes: u64) -> io::Result<Self> {
        let dir = dir.into();
        // ponytail: `create_dir`, not `create_dir_all` — the recursive variant drags in
        // `Path::components` machinery (~3 KiB) for a parent the host always provides.
        match fs::create_dir(&dir) {
            Err(err) if err.kind() != io::ErrorKind::AlreadyExists => return Err(err),
            _ => {}
        }
        let cache = Self {
            dir,
            max_bytes,
            max_batches: MAX_BATCHES,
            evicted: Mutex::new(Vec::new()),
            recovery_failed: Default::default(),
            #[cfg(test)]
            recoveries: Default::default(),
            #[cfg(test)]
            fault: Mutex::new(None),
        };
        cache.recover()?;
        {
            // Under `SPLITS` too: another cache's split in progress has `.tmp` files here.
            let _splits = splits();
            for entry in fs::read_dir(&cache.dir)?.flatten() {
                let path = entry.path();
                if !is_batch(&path) && path.extension().is_none_or(|e| e != PEND) {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        let evicted = cache.prune()?;
        *cache.evicted.lock().unwrap_or_else(|e| e.into_inner()) = evicted;
        Ok(cache)
    }

    /// A cache with a lower file-count bound (tests only).
    #[cfg(test)]
    pub(crate) fn open_with_max_batches(
        dir: impl Into<PathBuf>,
        max_bytes: u64,
        max_batches: usize,
    ) -> io::Result<Self> {
        let mut cache = Self::open(dir, max_bytes)?;
        cache.max_batches = max_batches;
        Ok(cache)
    }

    /// Fail or crash at storage steps from now on (tests only).
    #[cfg(test)]
    pub(crate) fn inject(
        &self,
        fault: impl Fn(Step, &Path) -> io::Result<()> + Send + Sync + 'static,
    ) {
        *self.fault.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(fault));
    }

    #[allow(unused_variables)]
    fn step(&self, step: Step, path: &Path) -> io::Result<()> {
        #[cfg(test)]
        if let Some(fault) = self.fault.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            return fault(step, path);
        }
        Ok(())
    }

    fn path(&self, id: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{id}.{ext}"))
    }

    /// A replacement half journaled under its parent: `<parent>@<half>.pend`.
    fn pend(&self, parent: &str, half: &str) -> PathBuf {
        self.dir.join(format!("{parent}{JOURNAL}{half}.{PEND}"))
    }

    /// The journal files in the directory.
    fn journal(&self) -> io::Result<Vec<PathBuf>> {
        Ok(fs::read_dir(&self.dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == PEND))
            .collect())
    }

    /// Write `body` to `tmp` and flush it to the device (`fsync`), so the rename that publishes
    /// it can never expose a file whose contents are still only in the page cache.
    fn write_synced(&self, tmp: &Path, body: &[u8]) -> io::Result<()> {
        let write = |path: &Path| -> io::Result<()> {
            self.step(Step::Write, path)?;
            let mut file = fs::File::create(path)?;
            io::Write::write_all(&mut file, body)?;
            self.step(Step::Sync, path)?;
            file.sync_all()
        };
        match write(tmp) {
            // iOS may purge the whole Caches subdirectory while the app runs: recreate it once.
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&self.dir)?;
                write(tmp)
            }
            other => other,
        }
    }

    fn rename(&self, from: &Path, to: &Path, step: Step) -> io::Result<()> {
        self.step(step, to)?;
        fs::rename(from, to)
    }

    /// Make the directory's entries (renames, deletes) durable. Failures are errors; only a
    /// platform that cannot sync a directory at all (not Unix) skips it.
    fn sync_dir(&self) -> io::Result<()> {
        self.step(Step::SyncDir, &self.dir)?;
        #[cfg(unix)]
        fs::File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    /// The batch ids in the directory, oldest first; an unreadable directory is an error. A
    /// split committed but not yet published (its publish failed) is published first, so its
    /// halves are pending — uploaded, purged and counted — like any other batch.
    fn ids(&self) -> io::Result<Vec<String>> {
        let list = || -> io::Result<(Vec<String>, bool)> {
            let (mut ids, mut journaled) = (Vec::new(), false);
            for path in fs::read_dir(&self.dir)?.flatten().map(|e| e.path()) {
                journaled |= path.extension().is_some_and(|e| e == PEND);
                if let Some(id) =
                    path.file_stem().and_then(|s| s.to_str()).filter(|_| is_batch(&path))
                {
                    ids.push(id.to_owned());
                }
            }
            Ok((ids, journaled))
        };
        let (mut ids, journaled) = list()?;
        if journaled && !self.recovery_failed.load(Ordering::Relaxed) {
            match self.recover() {
                Ok(()) => ids = list()?.0,
                Err(err) => {
                    self.recovery_failed.store(true, Ordering::Relaxed);
                    log::debug!("journaled split not published yet: {err}; retried after a write");
                }
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Finish or roll back a split a crash interrupted. Its halves are journaled as
    /// `<parent>@<half>.pend` before the parent is deleted (the commit point): with the parent
    /// still there the journal is discarded (the parent is whole); without it the halves are
    /// published. A rebinding — one half whose id extends its host-less parent's with the project
    /// the parent lacks — is rolled forward instead: a journal file is complete once it exists (its
    /// contents are `fsync`ed before the rename), and only it records the project. Either way
    /// every record is there exactly once. Under [`SPLITS`], so no split is in progress: a
    /// journal next to its parent is a crash's or a failed rollback's.
    fn recover(&self) -> io::Result<()> {
        let _splits = splits();
        #[cfg(test)]
        self.recoveries.fetch_add(1, Ordering::Relaxed);
        let journal = self.journal()?;
        if journal.is_empty() {
            return Ok(());
        }
        for pend in journal {
            let Some((parent, half)) =
                pend.file_stem().and_then(|s| s.to_str()).and_then(|s| s.split_once(JOURNAL))
            else {
                let _ = fs::remove_file(&pend);
                continue;
            };
            // A host-less id ends with its empty host; a split's halves rename the sequence.
            let rebinding =
                parent.ends_with('-') && half.len() > parent.len() && half.starts_with(parent);
            if self.path(parent, EXT).exists() {
                if !rebinding {
                    fs::remove_file(&pend)?;
                    continue;
                }
                // Parent first: a crash in between leaves a committed journal, published below.
                fs::remove_file(self.path(parent, EXT))?;
            }
            fs::rename(&pend, self.path(half, EXT))?;
        }
        self.sync_dir()
    }

    /// Delete the oldest batches until the rest fit `max_bytes` and the file-count bound (a
    /// committed split is published by the listing first). Returns the ids actually removed.
    fn prune(&self) -> io::Result<Vec<String>> {
        let mut removed = Vec::new();
        let kept = self.ids()?;
        let sizes: Vec<u64> = kept
            .iter()
            .map(|id| fs::metadata(self.path(id, EXT)).map(|m| m.len()).unwrap_or(0))
            .collect();
        let mut total: u64 = sizes.iter().sum();
        let mut count = kept.len();
        for (id, len) in kept.iter().zip(sizes) {
            if total <= self.max_bytes && count <= self.max_batches {
                break;
            }
            // A batch that could not be deleted still takes its room: keep evicting, report only
            // what really went.
            if self.remove(id).is_ok() {
                total -= len;
                count -= 1;
                removed.push(id.clone());
            }
        }
        Ok(removed)
    }

    /// The journaled section of [`BatchCache::replace`], under [`SPLITS`]. `Err` only before the
    /// commit point, with `old` whole.
    fn split(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<()> {
        let _splits = splits();
        let pends: Vec<PathBuf> = new.iter().map(|(half, _)| self.pend(old, half)).collect();
        let roll_back = |err: io::Error| {
            for ((half, _), pend) in new.iter().zip(&pends) {
                let _ = fs::remove_file(self.path(half, "tmp"));
                let _ = fs::remove_file(pend);
            }
            Err(err)
        };
        for ((half, body), pend) in new.iter().zip(&pends) {
            let tmp = self.path(half, "tmp");
            if let Err(err) =
                self.write_synced(&tmp, body).and_then(|()| self.rename(&tmp, pend, Step::Rename))
            {
                return roll_back(err);
            }
        }
        if let Err(err) = self.sync_dir() {
            return roll_back(err);
        }
        if let Err(err) = self
            .step(Step::Delete, &self.path(old, EXT))
            .and_then(|()| fs::remove_file(self.path(old, EXT)))
        {
            return roll_back(err);
        }
        // Committed: the records are the halves now, whatever follows. A failure from here leaves
        // them journaled — published by the next listing (`ids`), purged by an opt-out — so the
        // split succeeded and must not be read as "the parent is still there".
        let published = self
            .sync_dir()
            .and_then(|()| {
                new.iter().zip(&pends).try_for_each(|((half, _), pend)| {
                    self.rename(pend, &self.path(half, EXT), Step::Publish)
                })
            })
            .and_then(|()| self.sync_dir());
        if let Err(err) = published {
            log::warn!("split committed but not published yet ({err}); it will be");
        }
        Ok(())
    }
}

/// The durability boundary: a batch is committed once `push` returns `Ok` — its file was written
/// to a `.tmp`, `fsync`ed, renamed into place and the directory `fsync`ed; if any of that fails
/// the file is removed and `push` errs (the caller keeps the batch in memory instead). A crash
/// before that point loses only the batch being written (its `.tmp` is removed at the next
/// open). Deletes are `fsync`ed the same way. A split (`replace`) is journaled: see
/// [`recover`](FileCache::recover).
impl BatchCache for FileCache {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        let (tmp, dest) = (self.path(id, "tmp"), self.path(id, EXT));
        let written = self
            .write_synced(&tmp, body)
            .and_then(|()| self.rename(&tmp, &dest, Step::Rename))
            .and_then(|()| self.sync_dir());
        if let Err(err) = written {
            // Not committed: nothing of it may stay (ENOSPC leaves a truncated `.tmp`, a failed
            // directory sync a published file of unknown durability).
            let _ = fs::remove_file(&tmp);
            let _ = fs::remove_file(&dest);
            return Err(err);
        }
        // Committed. Eviction trouble must not turn that into an error (the caller would keep a
        // second copy in memory): it is logged, and the bound is enforced at the next push.
        self.recovery_failed.store(false, Ordering::Relaxed);
        Ok(self.prune().unwrap_or_else(|err| {
            log::debug!("could not prune the cache: {err}");
            Vec::new()
        }))
    }

    /// Journaled: both halves are written, `fsync`ed and renamed to `<old>@<half>.pend`, the
    /// directory is `fsync`ed, then `old` is deleted — the commit point — and the halves are
    /// published. Before the commit point any failure rolls back and `old` stays whole; after it
    /// the halves are safe in the journal and published by the next listing. Nothing is evicted
    /// until the replacement is complete.
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        self.split(old, new)?;
        self.recovery_failed.store(false, Ordering::Relaxed);
        Ok(self.prune().unwrap_or_default())
    }

    fn pending(&self) -> Vec<String> {
        self.ids().unwrap_or_default()
    }

    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        fs::read(self.path(id, EXT))
    }

    fn remove(&self, id: &str) -> io::Result<()> {
        let path = self.path(id, EXT);
        self.step(Step::Delete, &path)?;
        match fs::remove_file(&path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => self.sync_dir(),
        }
    }

    fn clear(&self) -> io::Result<()> {
        // Try every file — batches and journaled split halves — then report the first failure,
        // an unreadable directory included.
        let mut first = Ok(());
        for id in self.ids()? {
            if let Err(err) = self.remove(&id) {
                first = first.and(Err(err));
            }
        }
        for pend in self.journal()? {
            if let Err(err) = fs::remove_file(&pend) {
                first = first.and(Err(err));
            }
        }
        first.and_then(|()| self.sync_dir())
    }

    fn take_evicted(&self) -> Vec<String> {
        std::mem::take(&mut *self.evicted.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// A cache that never refuses a batch: what the primary cannot store (disk full, directory
/// gone) goes to memory instead, so it still uploads — it just no longer survives the process.
/// Every spill is counted as `cache_write_errors`.
pub(crate) struct SpillCache {
    primary: std::sync::Arc<dyn BatchCache>,
    spill: MemoryCache,
    counters: std::sync::Arc<crate::stats::Counters>,
    /// The consent barrier at the commit point: writes check the opt-out under `gate`, and
    /// `clear` takes `gate` too, so no write can land after a purge has cleared.
    revoked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    gate: Mutex<()>,
}

impl SpillCache {
    pub fn new(
        primary: std::sync::Arc<dyn BatchCache>,
        max_bytes: u64,
        counters: std::sync::Arc<crate::stats::Counters>,
        revoked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            primary,
            spill: MemoryCache::new(max_bytes),
            counters,
            revoked,
            gate: Mutex::new(()),
        }
    }

    /// Hold the write gate; `Err` once the app opted out.
    fn open_gate(&self) -> io::Result<std::sync::MutexGuard<'_, ()>> {
        let gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.revoked.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(io::Error::other("telemetry was disabled"));
        }
        Ok(gate)
    }
}

impl BatchCache for SpillCache {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        let _gate = self.open_gate()?;
        match self.primary.push(id, body) {
            Ok(evicted) => Ok(evicted),
            Err(err) => {
                let first = self.counters.snapshot().cache_write_errors == 0;
                crate::stats::Counters::add(&self.counters.cache_write_errors, 1);
                if first {
                    log::warn!(
                        "cannot write the telemetry cache ({err}); keeping batches in memory"
                    );
                }
                self.spill.push(id, body)
            }
        }
    }

    /// A batch spilled to memory is split in memory; one on disk is split on disk or not at all
    /// — never moved from disk to memory, where a crash would lose it.
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        let _gate = self.open_gate()?;
        if self.spill.pending().iter().any(|id| id == old) {
            self.spill.replace(old, new)
        } else {
            self.primary.replace(old, new)
        }
    }

    fn pending(&self) -> Vec<String> {
        let mut ids = self.primary.pending();
        ids.extend(self.spill.pending());
        ids.sort_unstable();
        ids
    }

    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        self.spill.read(id).or_else(|_| self.primary.read(id))
    }

    fn remove(&self, id: &str) -> io::Result<()> {
        self.spill.remove(id)?;
        self.primary.remove(id)
    }

    fn clear(&self) -> io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        self.spill.clear()?;
        self.primary.clear()
    }

    fn take_evicted(&self) -> Vec<String> {
        self.primary.take_evicted()
    }
}

/// A batch file: our extension and a creation stamp in its name. Anything else in the directory
/// (a half-written `.tmp`, a stray or mangled file) is removed when the cache is pruned.
fn is_batch(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXT) && stamp(path).is_some()
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
    use std::sync::Arc;

    use super::*;

    /// Ids the exporter would mint: a real timestamp (fixed for the test run, so ids stay
    /// stable however slow the disk) + seq.
    fn id(n: u64) -> String {
        static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        let base =
            *BASE.get_or_init(|| crate::event::now_unix_nanos() / 1_000_000_000 * 1_000_000_000);
        format!("{:020}-{n:06}-1", base + n)
    }

    #[test]
    fn memory_cache_evicts_oldest_beyond_max_bytes() {
        let cache = MemoryCache::new(25);
        for (n, body) in [b"aaaaaaaaaa", b"bbbbbbbbbb", b"cccccccccc"].into_iter().enumerate() {
            cache.push(&id(n as u64), body).expect("push");
        }
        assert_eq!(cache.pending(), [id(1), id(2)]);
        assert_eq!(cache.read(&id(1)).expect("read"), b"bbbbbbbbbb");
        cache.remove(&id(1)).expect("remove");
        assert_eq!(cache.pending(), [id(2)]);
        cache.clear().expect("clear");
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
        // A stray `.tmp` goes at the next open, never mid-run: it may be a replaced pipeline's
        // write in progress.
        assert!(dir.join("crashed.tmp").exists());
        let cache = FileCache::open(&dir, 25).expect("reopen");
        assert!(!dir.join("crashed.tmp").exists());
        cache.clear().expect("clear");
        assert!(cache.pending().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Days offline at a one-second cadence are tiny batches, not big ones: the byte cap alone
    /// would leave thousands of files in the directory.
    #[test]
    fn file_cache_caps_the_number_of_batches() {
        let dir = temp_dir("count");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        // `id` stamps the current second, so keep the ids rather than recomputing them.
        let ids: Vec<String> = (0..(MAX_BATCHES as u64 + 8)).map(id).collect();
        for batch in &ids {
            cache.push(batch, b"x").expect("push");
        }
        let kept = cache.pending();
        assert_eq!(kept.len(), MAX_BATCHES, "far under the byte cap, still bounded");
        assert_eq!(kept.first(), Some(&ids[8]), "the oldest went first");
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

    /// Findings r1-7 / r2-7 / r2-12: a real split, crashed at every one of its storage steps and
    /// reopened under byte and file-count pressure, leaves every record exactly once — the
    /// parent whole, or both halves — and recovery runs before anything is evicted.
    #[test]
    fn a_split_crashed_at_every_step_loses_and_duplicates_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (parent, a, b) = (id(1), format!("{}a", id(1)), format!("{}b", id(1)));
        let halves = [(a.clone(), vec![b'a'; 60]), (b.clone(), vec![b'b'; 60])];
        // Room for the parent and not a byte more / not a file more: a duplicate would force an
        // eviction.
        // (The split's two halves take two files: the file bound is two — a duplicate would
        // need three.)
        for (max_bytes, max_batches) in [(120, MAX_BATCHES), (1 << 20, 2)] {
            let mut step = 0;
            loop {
                let dir = temp_dir(&format!("split-{step}-{max_batches}"));
                let cache =
                    FileCache::open_with_max_batches(&dir, max_bytes, max_batches).expect("open");
                cache.push(&parent, &[b'p'; 120]).expect("push");
                let calls = Arc::new(AtomicUsize::new(0));
                let (counter, crash_at) = (calls.clone(), step);
                cache.inject(move |_, _| {
                    if counter.fetch_add(1, Ordering::SeqCst) == crash_at {
                        panic!("killed");
                    }
                    Ok(())
                });
                let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = cache.replace(&parent, &halves);
                }))
                .is_err();
                drop(cache);
                let cache =
                    FileCache::open_with_max_batches(&dir, max_bytes, max_batches).expect("reopen");
                assert!(cache.take_evicted().is_empty(), "step {step}: nothing evicted");
                let pending = cache.pending();
                let whole = pending == [parent.clone()]
                    && cache.read(&parent).expect("parent").len() == 120;
                let split = pending == [a.clone(), b.clone()];
                assert!(whole || split, "step {step}: {pending:?}");
                let _ = fs::remove_dir_all(&dir);
                if !crashed {
                    assert!(split, "a replace that ran to the end split the batch");
                    break;
                }
                step += 1;
            }
            assert!(
                step >= 10,
                "every write, sync, rename, delete and publish was crashed at: {step}"
            );
        }
    }

    /// Finding r3-2: a split that fails after its commit point (a failed publish, a failed
    /// directory sync) has succeeded — its halves are safe in the journal and published by the
    /// next listing — and an opt-out deletes the journal too, leaving the directory empty.
    #[test]
    fn a_split_failing_after_its_commit_point_is_kept_and_purgeable() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let (parent, a, b) = (id(1), format!("{}a", id(1)), format!("{}b", id(1)));
        let halves = [(a.clone(), b"half a".to_vec()), (b.clone(), b"half b".to_vec())];
        for failing in [Step::Publish, Step::SyncDir] {
            let dir = temp_dir(&format!("post-commit-{failing:?}"));
            let cache = FileCache::open(&dir, 1 << 20).expect("open");
            cache.push(&parent, b"parent").expect("push");
            let committed = Arc::new(AtomicBool::new(false));
            let seen = committed.clone();
            cache.inject(move |step, _| {
                if step == Step::Delete {
                    seen.store(true, Ordering::SeqCst);
                }
                if step == failing && seen.load(Ordering::SeqCst) {
                    return Err(io::Error::other("injected"));
                }
                Ok(())
            });
            let ids = cache.replace(&parent, &halves).expect("committed: not an error");
            assert!(!cache.pending().contains(&parent), "{failing:?}: the parent is gone");
            cache.inject(|_, _| Ok(())); // the storage recovers; then the app opts out
            assert!(cache.clear().is_ok());
            assert_eq!(
                fs::read_dir(&dir).expect("dir").count(),
                0,
                "{failing:?}: nothing survives the opt-out"
            );
            let _ = ids;

            // Without the opt-out, the next listing publishes the journaled halves.
            let dir2 = temp_dir(&format!("post-commit-publish-{failing:?}"));
            let cache = FileCache::open(&dir2, 1 << 20).expect("open");
            cache.push(&parent, b"parent").expect("push");
            let flag = Arc::new(AtomicBool::new(true));
            let fail = flag.clone();
            let committed = Arc::new(AtomicBool::new(false));
            let seen = committed.clone();
            cache.inject(move |step, _| {
                if step == Step::Delete {
                    seen.store(true, Ordering::SeqCst);
                }
                if step == failing && seen.load(Ordering::SeqCst) && fail.load(Ordering::SeqCst) {
                    return Err(io::Error::other("injected"));
                }
                Ok(())
            });
            cache.replace(&parent, &halves).expect("committed");
            flag.store(false, Ordering::SeqCst);
            cache.push(&id(9), b"next").expect("push");
            assert_eq!(cache.pending(), [a.clone(), b.clone(), id(9)], "{failing:?}: published");
            let _ = fs::remove_dir_all(&dir);
            let _ = fs::remove_dir_all(&dir2);
        }
    }

    /// Finding r4-NB: a reconfigure opens a second cache on the directory while the replaced
    /// pipeline still drains. Its open, pushes and listings must never see that pipeline's split
    /// half-done — deleting the journal of a split about to commit would lose both halves.
    #[test]
    fn a_second_cache_on_the_directory_never_breaks_a_split_in_progress() {
        let dir = temp_dir("two-generations");
        let (parent, a, b) = (id(1), format!("{}a", id(1)), format!("{}b", id(1)));
        let halves = [(a.clone(), b"half a".to_vec()), (b.clone(), b"half b".to_vec())];
        let old = FileCache::open(&dir, 1 << 20).expect("open");
        old.push(&parent, b"parent").expect("push");
        let other = Arc::new(Mutex::new(None));
        let (handle, path) = (other.clone(), dir.clone());
        old.inject(move |step, _| {
            // Journaled, parent still there, commit next: the new generation starts now.
            let mut handle = handle.lock().expect("lock");
            if step == Step::Delete && handle.is_none() {
                let path = path.clone();
                *handle = Some(std::thread::spawn(move || {
                    let new = FileCache::open(&path, 1 << 20).expect("open");
                    new.push(&id(2), b"new generation").expect("push");
                }));
                // Give it every chance to reach the directory mid-split.
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Ok(())
        });
        old.replace(&parent, &halves).expect("split");
        let new = other.lock().expect("lock").take().expect("the new generation ran");
        new.join().expect("the new generation succeeded");
        let reopened = FileCache::open(&dir, 1 << 20).expect("reopen");
        assert_eq!(reopened.pending(), [a, b, id(2)], "every record exactly once");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Round-5 review: a journal that cannot be published is not retried (and logged) by every
    /// listing — only again after the cache has written something.
    #[cfg(unix)]
    #[test]
    fn a_stuck_journal_is_retried_after_a_write_not_every_listing() {
        let dir = temp_dir("stuck-journal");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        let (parent, half) = (id(1), format!("{}a", id(1)));
        fs::write(cache.pend(&parent, &half), b"half").expect("journal");
        // The half's name is taken by a non-empty directory: the publishing rename fails.
        fs::create_dir(cache.path(&half, EXT)).expect("blocker");
        fs::write(cache.path(&half, EXT).join("x"), b"").expect("blocker content");
        let before = cache.recoveries.load(Ordering::Relaxed);
        for _ in 0..5 {
            cache.pending();
        }
        assert_eq!(cache.recoveries.load(Ordering::Relaxed), before + 1, "tried once");
        cache.push(&id(2), b"next").expect("push");
        assert_eq!(cache.recoveries.load(Ordering::Relaxed), before + 2, "a write retries");
        fs::remove_dir_all(cache.path(&half, EXT)).expect("unblock");
        cache.push(&id(3), b"next").expect("push");
        assert_eq!(cache.pending(), [half, id(2), id(3)], "published once unblocked");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Finding r2-12: a write killed half-way (bytes on disk, no rename) leaves no partial batch
    /// and costs no committed one.
    #[test]
    fn a_write_killed_half_way_leaves_no_partial_batch() {
        let dir = temp_dir("torn");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        cache.push(&id(1), b"committed").expect("push");
        cache.inject(|step, path| {
            if step == Step::Write {
                fs::write(path, b"half of a batc").expect("partial");
                panic!("killed mid-write");
            }
            Ok(())
        });
        let killed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = cache.push(&id(2), b"half of a batch that never lands");
        }));
        assert!(killed.is_err());
        drop(cache);
        let cache = FileCache::open(&dir, 1 << 20).expect("reopen");
        assert_eq!(cache.pending(), [id(1)]);
        assert_eq!(cache.read(&id(1)).expect("read"), b"committed");
        assert_eq!(fs::read_dir(&dir).expect("dir").count(), 1, "the torn .tmp is gone");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Finding r2-8: a push whose file or directory cannot be made durable is not committed —
    /// it errs and leaves nothing behind — and a split that cannot be made durable before its
    /// commit point keeps the parent.
    #[test]
    fn durability_failures_are_errors_not_silent_successes() {
        for failing in [Step::Sync, Step::SyncDir] {
            let dir = temp_dir("sync");
            let cache = FileCache::open(&dir, 1 << 20).expect("open");
            cache.push(&id(1), b"parent").expect("push");
            cache.inject(move |step, _| {
                if step == failing {
                    return Err(io::Error::other("fsync failed"));
                }
                Ok(())
            });
            assert!(cache.push(&id(2), b"new").is_err(), "{failing:?}: not committed");
            assert_eq!(cache.pending(), [id(1)], "{failing:?}: nothing left of it");
            let halves =
                [(format!("{}a", id(1)), b"a".to_vec()), (format!("{}b", id(1)), b"b".to_vec())];
            assert!(cache.replace(&id(1), &halves).is_err());
            assert_eq!(cache.pending(), [id(1)], "{failing:?}: the parent stays whole");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    /// Finding r2-8: an unreadable directory is an error for `clear`, not an empty success.
    #[cfg(unix)]
    #[test]
    fn clearing_an_unreadable_directory_fails() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("unreadable");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        cache.push(&id(1), b"batch").expect("push");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o300)).expect("chmod");
        assert!(cache.clear().is_err(), "cannot list, cannot claim it is empty");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(cache.pending(), [id(1)]);
        let _ = fs::remove_dir_all(&dir);
    }
}
