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
    cell::RefCell,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

/// Queue of encoded OTLP batches between the exporter (`Exporter`) and the transport.
///
/// The exporter writes every batch here *before* trying the network (write-ahead), uploads
/// oldest-first, and removes what the collector accepted or rejected. Two implementations ship:
/// [`MemoryCache`] (default) and [`FileCache`] (survives crashes and restarts); anything else —
/// a database, a platform store — plugs in through `Telemetry::with_cache`.
///
/// Ids are chosen by the exporter as `<unix ns>-<seq>-<records>-<l|t>-<session>-<project key>`.
/// They hold digits, `a` and `b` (a split's halves), hex or `process`, `l` or `t`, `-`, and a
/// routing key of lowercase letters, digits, `.`, `-`, `_`, `+`, and `%` followed by two uppercase
/// hex digits: characters every platform allows in a file name, Windows included. A batch without a
/// project ends with `-`, because its key is empty. Ids are sortable, so [`pending`](Self::pending)
/// is a plain sort, and prefixed with the creation time so an implementation can expire old batches
/// without touching file timestamps.
/// Implementations bound their own footprint by evicting the oldest batches and report what they
/// evicted, so every loss is counted. A failed write or delete is an `Err` and is never reported
/// as done, with one exception: [`FileCache`] counts an eviction once the file is gone, even when
/// the directory sync after it failed. A failed listing reads as empty, and the next pass tries
/// again.
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
/// bounded by `max_bytes` and 512 batches (`MAX_BATCHES`, oldest evicted). The newest batch always
/// stays, even when it alone is larger than `max_bytes`. Lost with the process.
// ponytail: a Vec with remove(0) — a handful of small batches, and it shares Vec code the
// binary already has instead of pulling in VecDeque's ring-buffer instantiations.
pub struct MemoryCache {
    batches: Mutex<Vec<(String, Vec<u8>)>>,
    max_bytes: usize,
}

impl MemoryCache {
    /// An empty cache holding at most `max_bytes` of batches, apart from the newest batch, which
    /// always stays.
    pub fn new(max_bytes: u64) -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
            max_bytes: usize::try_from(max_bytes).unwrap_or(usize::MAX),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, Vec<u8>)>> {
        self.batches.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Drop the oldest batches until the rest fit; returns their ids. The newest always stays.
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

/// The extension of a committed batch file.
const BATCH_EXT: &str = "otlp";
/// The extension of a file being written; a crash leaves it behind, and `FileCache::open` removes
/// it.
const TMP_EXT: &str = "tmp";

/// On-disk [`BatchCache`]: one file per encoded batch, so a crash or an offline shutdown loses
/// nothing and the next launch replays what is left.
///
/// Files are written as `.tmp` and renamed into place, so a crash never leaves a half batch
/// readable; eviction is drop-oldest above `max_bytes` or 512 files (`MAX_BATCHES`), and the
/// newest batch always stays. Age is the exporter's call (24 h, judged by the timestamp in the id
/// rather than file metadata — an Apple "required reason" API).
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

/// The extension of a journaled split half: `<parent>@<half>.pend`.
const JOURNAL_EXT: &str = "pend";
/// Separates parent and half in a journal file name. Batch ids never contain it (see the id
/// alphabet on [`BatchCache`]).
const JOURNAL_SEP: char = '@';

/// Held across a push's write, a split's journaled section and recovery, by every [`FileCache`]
/// in the process: a reconfigure opens a second cache on the directory while the replaced
/// pipeline still drains. Its open must never sweep away that pipeline's `.tmp` mid-write, and
/// its recovery must never see a split half-done — it would delete the journal of a split about
/// to commit, losing the halves.
// ponytail: process-wide rather than per directory. Every push and split holds it for a few
// fsyncs, and it contends only while two pipelines overlap in a reconfigure. One lock per
// directory if that ever matters.
static DIR_LOCK: Mutex<()> = Mutex::new(());

fn lock_dir() -> std::sync::MutexGuard<'static, ()> {
    DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner())
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
            // Under `DIR_LOCK` too: another cache's push or split in progress has `.tmp` files
            // here.
            let _locked = lock_dir();
            for entry in fs::read_dir(&cache.dir)?.flatten() {
                let path = entry.path();
                if !is_batch(&path) && path.extension().is_none_or(|e| e != JOURNAL_EXT) {
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

    /// Run before a storage step: in tests the injected fault can fail it, otherwise a no-op.
    #[cfg_attr(not(test), expect(unused_variables, reason = "only the test fault hook reads them"))]
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
        self.dir.join(format!("{parent}{JOURNAL_SEP}{half}.{JOURNAL_EXT}"))
    }

    /// The journal files in the directory.
    fn journal(&self) -> io::Result<Vec<PathBuf>> {
        Ok(fs::read_dir(&self.dir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == JOURNAL_EXT))
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

    /// After a committed write: listings may retry a stuck journal again, and the bounds are
    /// enforced. Eviction trouble is logged and never an error, because the write itself is
    /// committed (the caller would otherwise keep a second copy in memory); the next write
    /// enforces the bound again.
    fn after_write(&self) -> Vec<String> {
        self.recovery_failed.store(false, Ordering::Relaxed);
        self.prune().unwrap_or_else(|err| {
            diagnostic(log::Level::Debug, format!("could not prune the cache: {err}"));
            Vec::new()
        })
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
                journaled |= path.extension().is_some_and(|e| e == JOURNAL_EXT);
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
                    diagnostic(
                        log::Level::Debug,
                        format!("journaled split not published yet: {err}; retried after a write"),
                    );
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
    /// every record is there exactly once. Under [`DIR_LOCK`], so no split is in progress: a
    /// journal next to its parent is a crash's or a failed rollback's.
    fn recover(&self) -> io::Result<()> {
        let _locked = lock_dir();
        #[cfg(test)]
        self.recoveries.fetch_add(1, Ordering::Relaxed);
        let journal = self.journal()?;
        if journal.is_empty() {
            return Ok(());
        }
        for pend in journal {
            let Some((parent, half)) =
                pend.file_stem().and_then(|s| s.to_str()).and_then(|s| s.split_once(JOURNAL_SEP))
            else {
                let _ = fs::remove_file(&pend);
                continue;
            };
            let rebinding = is_rebinding(parent, half);
            if self.path(parent, BATCH_EXT).exists() {
                if !rebinding {
                    fs::remove_file(&pend)?;
                    continue;
                }
                // Parent first: a crash in between leaves a committed journal, published below.
                fs::remove_file(self.path(parent, BATCH_EXT))?;
            }
            fs::rename(&pend, self.path(half, BATCH_EXT))?;
        }
        self.sync_dir()
    }

    /// Delete the oldest batches until the rest fit `max_bytes` and the file-count bound (a
    /// committed split is published by the listing first). The newest batch always stays, as in
    /// [`MemoryCache`], so it gets its upload attempt. Returns the ids actually removed. An id
    /// counts once its file is gone, even when the directory sync after the unlink failed: the
    /// count follows the visible removal and does not promise a delete that survives a crash. A
    /// power loss can bring the file back, and the next open evicts it again or keeps it for
    /// upload, like any other batch.
    fn prune(&self) -> io::Result<Vec<String>> {
        let mut removed = Vec::new();
        let kept = self.ids()?;
        let sizes: Vec<u64> = kept
            .iter()
            .map(|id| fs::metadata(self.path(id, BATCH_EXT)).map(|m| m.len()).unwrap_or(0))
            .collect();
        let mut total: u64 = sizes.iter().sum();
        let mut count = kept.len();
        for (id, len) in kept.iter().zip(sizes) {
            if (total <= self.max_bytes && count <= self.max_batches) || count == 1 {
                break;
            }
            // A batch that could not be deleted still takes its room: keep evicting, report only
            // what really went. A delete whose directory sync failed still went.
            if self.remove(id).is_ok() || matches!(self.path(id, BATCH_EXT).try_exists(), Ok(false))
            {
                total -= len;
                count -= 1;
                removed.push(id.clone());
            }
        }
        Ok(removed)
    }

    /// The journaled section of [`BatchCache::replace`], under [`DIR_LOCK`]. `Err` only before the
    /// commit point, with `old` whole. A rebinding has exactly one member: recovery could not
    /// tell a crashed multi-member rebinding from a finished one.
    fn split(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<()> {
        if new.len() > 1 && new.iter().any(|(half, _)| is_rebinding(old, half)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a rebinding replaces a batch with exactly one batch",
            ));
        }
        let locked = lock_dir();
        let pends: Vec<PathBuf> = new.iter().map(|(half, _)| self.pend(old, half)).collect();
        let roll_back = |err: io::Error| {
            for ((half, _), pend) in new.iter().zip(&pends) {
                let _ = fs::remove_file(self.path(half, TMP_EXT));
                let _ = fs::remove_file(pend);
            }
            Err(err)
        };
        for ((half, body), pend) in new.iter().zip(&pends) {
            let tmp = self.path(half, TMP_EXT);
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
            .step(Step::Delete, &self.path(old, BATCH_EXT))
            .and_then(|()| fs::remove_file(self.path(old, BATCH_EXT)))
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
                    self.rename(pend, &self.path(half, BATCH_EXT), Step::Publish)
                })
            })
            .and_then(|()| self.sync_dir());
        // Logged after the lock: a log line may call into the host.
        drop(locked);
        if let Err(err) = published {
            diagnostic(
                log::Level::Warn,
                format!("split committed but not published yet ({err}); it will be"),
            );
        }
        Ok(())
    }
}

/// The durability boundary: a batch is committed once `push` returns `Ok` — its file was written
/// to a `.tmp`, `fsync`ed, renamed into place and the directory `fsync`ed; if any of that fails
/// the file is removed and `push` errs (the caller keeps the batch in memory instead). A crash
/// before that point loses only the batch being written (its `.tmp` is removed at the next
/// open). Deletes are `fsync`ed the same way. A split (`replace`) is journaled: see
/// `FileCache::recover`.
impl BatchCache for FileCache {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        let (tmp, dest) = (self.path(id, TMP_EXT), self.path(id, BATCH_EXT));
        // Under `DIR_LOCK`: another cache opening on the directory sweeps stray `.tmp` files.
        let locked = lock_dir();
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
        drop(locked);
        Ok(self.after_write())
    }

    /// Journaled: both halves are written, `fsync`ed and renamed to `<old>@<half>.pend`, the
    /// directory is `fsync`ed, then `old` is deleted — the commit point — and the halves are
    /// published. Before the commit point any failure rolls back and `old` stays whole; after it
    /// the halves are safe in the journal and published by the next listing. Nothing is evicted
    /// until the replacement is complete. A rebinding (a host-less `old` that a half extends with
    /// its project) has exactly one member. Otherwise this returns `InvalidInput` and writes
    /// nothing.
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        self.split(old, new)?;
        Ok(self.after_write())
    }

    fn pending(&self) -> Vec<String> {
        self.ids().unwrap_or_default()
    }

    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        fs::read(self.path(id, BATCH_EXT))
    }

    fn remove(&self, id: &str) -> io::Result<()> {
        let path = self.path(id, BATCH_EXT);
        self.step(Step::Delete, &path)?;
        match fs::remove_file(&path) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            // A gone file is synced too (an earlier unlink's sync may have failed), but a gone
            // directory (the OS purged it) has nothing left to sync.
            _ => match self.sync_dir() {
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        }
    }

    fn clear(&self) -> io::Result<()> {
        // Try every file — batches and journaled split halves — then report the first failure,
        // an unreadable directory included. A directory the OS purged is already empty.
        let ids = match self.ids() {
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            other => other?,
        };
        let mut first = Ok(());
        for id in ids {
            if let Err(err) = self.remove(&id) {
                first = first.and(Err(err));
            }
        }
        for pend in self.journal()? {
            if let Err(err) = fs::remove_file(&pend) {
                first = first.and(Err(err));
            }
        }
        // A `.tmp` that a failed write could not remove still holds a batch. Under `DIR_LOCK`, so
        // no push or split is writing one right now.
        {
            let _locked = lock_dir();
            let entries = match fs::read_dir(&self.dir) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => return first,
                other => other?,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == TMP_EXT) {
                    if let Err(err) = fs::remove_file(&path) {
                        first = first.and(Err(err));
                    }
                }
            }
        }
        // Nothing journaled is left to publish.
        self.recovery_failed.store(false, Ordering::Relaxed);
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
    revoked: std::sync::Arc<AtomicBool>,
    gate: Mutex<()>,
    /// The primary is failing: the first spill of an episode warns, a successful write ends it.
    spilling: AtomicBool,
}

impl SpillCache {
    pub fn new(
        primary: std::sync::Arc<dyn BatchCache>,
        max_bytes: u64,
        counters: std::sync::Arc<crate::stats::Counters>,
        revoked: std::sync::Arc<AtomicBool>,
    ) -> Self {
        Self {
            primary,
            spill: MemoryCache::new(max_bytes),
            counters,
            revoked,
            gate: Mutex::new(()),
            spilling: AtomicBool::new(false),
        }
    }

    /// Hold the write gate; `Err` once the app opted out.
    fn open_gate(&self) -> io::Result<std::sync::MutexGuard<'_, ()>> {
        let gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.revoked.load(Ordering::SeqCst) {
            return Err(io::Error::other("telemetry was disabled"));
        }
        Ok(gate)
    }
}

impl BatchCache for SpillCache {
    fn push(&self, id: &str, body: &[u8]) -> io::Result<Vec<String>> {
        logged_after(|| {
            let gate = self.open_gate()?;
            let err = match self.primary.push(id, body) {
                Ok(evicted) => {
                    self.spilling.store(false, Ordering::Relaxed);
                    return Ok(evicted);
                }
                Err(err) => err,
            };
            crate::stats::Counters::add(&self.counters.cache_write_errors, 1);
            let first = !self.spilling.swap(true, Ordering::Relaxed);
            let spilled = self.spill.push(id, body);
            // Logged after the gate: a log line may call into the host.
            drop(gate);
            if first {
                log::warn!("cannot write the telemetry cache ({err}); keeping batches in memory");
            }
            spilled
        })
    }

    /// A batch spilled to memory is split in memory; one on disk is split on disk or not at all
    /// — never moved from disk to memory, where a crash would lose it.
    fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
        logged_after(|| {
            let _gate = self.open_gate()?;
            if self.spill.pending().iter().any(|id| id == old) {
                self.spill.replace(old, new)
            } else {
                self.primary.replace(old, new)
            }
        })
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
        logged_after(|| {
            let _gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
            self.spill.clear()?;
            self.primary.clear()
        })
    }

    fn take_evicted(&self) -> Vec<String> {
        self.primary.take_evicted()
    }
}

thread_local! {
    /// Diagnostics held back while this thread holds a lock a host logger must not find taken
    /// (the spill gate, the span registry); `None` logs them at once.
    static HELD_BACK: RefCell<Option<Vec<(log::Level, String)>>> = const { RefCell::new(None) };
}

/// Log `message` now, or once the lock this thread holds is released (see [`logged_after`]): a
/// host logger may call back into the pipeline, and must not find that lock taken.
pub(crate) fn diagnostic(level: log::Level, message: String) {
    let now = HELD_BACK.with(|held| match held.borrow_mut().as_mut() {
        Some(queue) => {
            queue.push((level, message));
            None
        }
        None => Some(message),
    });
    if let Some(message) = now {
        log::log!(level, "{message}");
    }
}

/// Run `locked`, which takes a lock, and log the diagnostics raised under it once it returned and
/// released that lock (into an outer `logged_after`, if any).
pub(crate) fn logged_after<T>(locked: impl FnOnce() -> T) -> T {
    struct Release(Option<Vec<(log::Level, String)>>);
    impl Drop for Release {
        fn drop(&mut self) {
            let held =
                HELD_BACK.with(|held| std::mem::replace(&mut *held.borrow_mut(), self.0.take()));
            for (level, message) in held.unwrap_or_default() {
                diagnostic(level, message);
            }
        }
    }
    let _release = Release(HELD_BACK.with(|held| held.borrow_mut().replace(Vec::new())));
    locked()
}

/// A batch file: our extension and a creation stamp in its name. Anything else in the directory
/// (a half-written `.tmp`, a stray or mangled file) is removed by [`FileCache::open`].
fn is_batch(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == BATCH_EXT)
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split('-').next())
            .is_some_and(|stamp| stamp.parse::<u64>().is_ok())
}

/// Whether `half` rebinds `parent`: it extends the host-less parent's id (which ends with its
/// empty host) with the project the parent lacks. A split's halves rename the sequence instead.
fn is_rebinding(parent: &str, half: &str) -> bool {
    parent.ends_with('-') && half.len() > parent.len() && half.starts_with(parent)
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
    fn the_newest_batch_always_stays_whatever_the_cap() {
        let dir = temp_dir("floor");
        let cache = FileCache::open(&dir, 25).expect("open");
        assert_eq!(cache.push(&id(1), &[b'a'; 30]).expect("push"), Vec::<String>::new());
        assert_eq!(cache.pending(), [id(1)], "a batch over the cap still gets its upload");
        assert_eq!(cache.push(&id(2), &[b'b'; 30]).expect("push"), [id(1)]);
        assert_eq!(cache.pending(), [id(2)]);
        let _ = fs::remove_dir_all(&dir);
        let dir = temp_dir("floor-zero");
        let cache = FileCache::open(&dir, 0).expect("open");
        cache.push(&id(1), b"x").expect("push");
        assert_eq!(cache.pending(), [id(1)], "a cap of 0 keeps the push");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_eviction_whose_directory_sync_fails_still_counts() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = temp_dir("evict-sync");
        let cache = FileCache::open(&dir, 10).expect("open");
        cache.push(&id(1), &[b'a'; 10]).expect("push");
        let deleted = Arc::new(AtomicBool::new(false));
        cache.inject(move |step, _| {
            if step == Step::Delete {
                deleted.store(true, Ordering::SeqCst);
            }
            if step == Step::SyncDir && deleted.load(Ordering::SeqCst) {
                return Err(io::Error::other("injected"));
            }
            Ok(())
        });
        assert_eq!(cache.push(&id(2), &[b'b'; 10]).expect("push"), [id(1)]);
        assert_eq!(cache.pending(), [id(2)], "the batch just committed stays");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_purged_directory_is_already_empty() {
        let dir = temp_dir("purged");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        cache.push(&id(1), b"x").expect("push");
        fs::remove_dir_all(&dir).expect("the OS purges the directory");
        assert!(cache.remove(&id(1)).is_ok());
        assert!(cache.clear().is_ok(), "an opt-out with nothing on disk succeeds");
    }

    #[test]
    fn a_rebinding_replaces_a_batch_with_exactly_one() {
        let dir = temp_dir("rebinding");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        let parent = format!("{}-l-s-", id(1));
        cache.push(&parent, b"parent").expect("push");
        let two = [
            (format!("{parent}a.example"), b"a".to_vec()),
            (format!("{parent}b.example"), b"b".to_vec()),
        ];
        let err = cache.replace(&parent, &two).expect_err("rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(cache.pending(), std::slice::from_ref(&parent), "the parent is kept");
        let one = [(format!("{parent}a.example"), b"a".to_vec())];
        cache.replace(&parent, &one).expect("one member");
        assert_eq!(cache.pending(), [format!("{parent}a.example")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_lets_listings_publish_a_journal_again() {
        let dir = temp_dir("clear-recovery");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        cache.recovery_failed.store(true, Ordering::Relaxed);
        cache.clear().expect("clear");
        assert!(!cache.recovery_failed.load(Ordering::Relaxed));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A second cache opening on the directory (a reconfigure) never sweeps away the `.tmp` of a
    /// push still in flight.
    #[test]
    fn a_second_cache_never_breaks_a_push_in_progress() {
        let dir = temp_dir("push-two-generations");
        let old = FileCache::open(&dir, 1 << 20).expect("open");
        let (opened, path) = (Arc::new(Mutex::new(None)), dir.clone());
        let handle = opened.clone();
        old.inject(move |step, _| {
            let mut handle = handle.lock().expect("lock");
            if step == Step::Rename && handle.is_none() {
                let path = path.clone();
                *handle = Some(std::thread::spawn(move || {
                    FileCache::open(&path, 1 << 20).expect("open");
                }));
                // Give it every chance to sweep the directory mid-push.
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Ok(())
        });
        old.push(&id(1), b"in flight").expect("the push commits");
        opened.lock().expect("lock").take().expect("opened").join().expect("open succeeded");
        assert_eq!(old.pending(), [id(1)]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A `.tmp` that a failed write could not remove (the directory went read-only) is deleted by
    /// the next `clear` once the directory is writable again: an opt-out leaves no batch bytes.
    #[cfg(unix)]
    #[test]
    fn clear_deletes_a_tmp_a_failed_write_left_behind() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("clear-tmp");
        let cache = FileCache::open(&dir, 1024).expect("open");
        cache.inject(|step, path| {
            if step == Step::Sync {
                let parent = path.parent().expect("the cache directory");
                fs::set_permissions(parent, fs::Permissions::from_mode(0o555))?;
                return Err(io::Error::other("disk gone"));
            }
            Ok(())
        });
        let pushed = cache.push(&id(1), b"telemetry bytes");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("writable again");
        cache.inject(|_, _| Ok(()));
        assert!(pushed.is_err());
        assert_eq!(fs::read_dir(&dir).expect("dir").count(), 1, "the failed write left its .tmp");
        cache.clear().expect("clear");
        let left: Vec<_> = fs::read_dir(&dir).expect("dir").flatten().map(|e| e.path()).collect();
        let _ = fs::remove_dir_all(&dir);
        assert!(left.is_empty(), "{left:?}");
    }

    /// A warning raised inside a gated write reaches the host logger only once the spill gate is
    /// released: a logger that calls back into the cache (a purge, say) does not deadlock.
    #[test]
    fn diagnostics_reach_the_logger_outside_the_spill_gate() {
        crate::test_log::capture();
        let dir = temp_dir("diagnostics-outside-the-gate");
        let primary = Arc::new(FileCache::open(&dir, 1024).expect("open"));
        primary.push(&id(1), b"parent").expect("push");
        primary.inject(|step, _| match step {
            Step::Publish => Err(io::Error::other("publish failed")),
            _ => Ok(()),
        });
        let cache = Arc::new(SpillCache::new(primary, 1024, Arc::default(), Arc::default()));
        let gated = Arc::downgrade(&cache);
        crate::test_log::mark_while(move || {
            gated.upgrade().is_some_and(|cache| cache.gate.try_lock().is_err())
        });
        let halves =
            [(format!("{}a", id(1)), b"a".to_vec()), (format!("{}b", id(1)), b"b".to_vec())];
        cache.replace(&id(1), &halves).expect("split");
        let lines = crate::test_log::take();
        let _ = fs::remove_dir_all(&dir);
        assert!(lines.iter().any(|l| l.contains("split committed but not published")), "{lines:?}");
        assert!(lines.iter().all(|l| !l.starts_with("held: ")), "{lines:?}");
    }

    /// A primary that fails while its flag is set.
    struct Flaky(std::sync::atomic::AtomicBool);

    impl BatchCache for Flaky {
        fn push(&self, _: &str, _: &[u8]) -> io::Result<Vec<String>> {
            if self.0.load(Ordering::SeqCst) {
                return Err(io::Error::other("disk full"));
            }
            Ok(Vec::new())
        }
        fn replace(&self, _: &str, _: &[(String, Vec<u8>)]) -> io::Result<Vec<String>> {
            Ok(Vec::new())
        }
        fn pending(&self) -> Vec<String> {
            Vec::new()
        }
        fn read(&self, _: &str) -> io::Result<Vec<u8>> {
            Err(io::ErrorKind::NotFound.into())
        }
        fn remove(&self, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn clear(&self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn each_spill_episode_warns_once() {
        crate::test_log::capture();
        let primary = Arc::new(Flaky(true.into()));
        let counters = Arc::new(crate::stats::Counters::default());
        let cache = SpillCache::new(primary.clone(), 1 << 20, counters.clone(), Arc::default());
        let warning = "cannot write the telemetry cache (disk full); keeping batches in memory";
        cache.push(&id(1), b"a").expect("spilled");
        cache.push(&id(2), b"b").expect("spilled");
        assert_eq!(crate::test_log::take(), [warning], "two spills, one warning");
        primary.0.store(false, Ordering::SeqCst);
        cache.push(&id(3), b"c").expect("written");
        primary.0.store(true, Ordering::SeqCst);
        cache.push(&id(4), b"d").expect("spilled");
        assert_eq!(crate::test_log::take(), [warning], "a new episode warns again");
        assert_eq!(counters.snapshot().cache_write_errors, 3);
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

    /// A real split, crashed at every one of its storage steps and
    /// reopened under byte and file-count pressure, leaves every record exactly once — the
    /// parent whole, or both halves — and recovery runs before anything is evicted.
    #[test]
    fn a_split_crashed_at_every_step_loses_and_duplicates_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (parent, a, b) = (id(1), format!("{}a", id(1)), format!("{}b", id(1)));
        let halves = [(a.clone(), vec![b'a'; 60]), (b.clone(), vec![b'b'; 60])];
        // Room for the parent and not a byte more, or for the two halves and not a file more: a
        // duplicate would force an eviction.
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

    /// A split that fails after its commit point (a failed publish, a failed
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
            let seen = Arc::new(AtomicBool::new(false));
            cache.inject(move |step, _| {
                if step == Step::Delete {
                    seen.store(true, Ordering::SeqCst);
                }
                if step == failing && seen.load(Ordering::SeqCst) {
                    return Err(io::Error::other("injected"));
                }
                Ok(())
            });
            cache.replace(&parent, &halves).expect("committed: not an error");
            assert!(!cache.pending().contains(&parent), "{failing:?}: the parent is gone");
            cache.inject(|_, _| Ok(())); // the storage recovers; then the app opts out
            assert!(cache.clear().is_ok());
            assert_eq!(
                fs::read_dir(&dir).expect("dir").count(),
                0,
                "{failing:?}: nothing survives the opt-out"
            );

            // Without the opt-out, the next listing publishes the journaled halves.
            let dir2 = temp_dir(&format!("post-commit-publish-{failing:?}"));
            let cache = FileCache::open(&dir2, 1 << 20).expect("open");
            cache.push(&parent, b"parent").expect("push");
            let flag = Arc::new(AtomicBool::new(true));
            let fail = flag.clone();
            let seen = Arc::new(AtomicBool::new(false));
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

    /// A reconfigure opens a second cache on the directory while the replaced
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

    /// A journal that cannot be published is not retried (and logged) by every
    /// listing — only again after the cache has written something.
    #[cfg(unix)]
    #[test]
    fn a_stuck_journal_is_retried_after_a_write_not_every_listing() {
        let dir = temp_dir("stuck-journal");
        let cache = FileCache::open(&dir, 1 << 20).expect("open");
        let (parent, half) = (id(1), format!("{}a", id(1)));
        fs::write(cache.pend(&parent, &half), b"half").expect("journal");
        // The half's name is taken by a non-empty directory: the publishing rename fails.
        fs::create_dir(cache.path(&half, BATCH_EXT)).expect("blocker");
        fs::write(cache.path(&half, BATCH_EXT).join("x"), b"").expect("blocker content");
        let before = cache.recoveries.load(Ordering::Relaxed);
        for _ in 0..5 {
            cache.pending();
        }
        assert_eq!(cache.recoveries.load(Ordering::Relaxed), before + 1, "tried once");
        cache.push(&id(2), b"next").expect("push");
        assert_eq!(cache.recoveries.load(Ordering::Relaxed), before + 2, "a write retries");
        fs::remove_dir_all(cache.path(&half, BATCH_EXT)).expect("unblock");
        cache.push(&id(3), b"next").expect("push");
        assert_eq!(cache.pending(), [half, id(2), id(3)], "published once unblocked");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A write killed half-way (bytes on disk, no rename) leaves no partial batch
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

    /// A push whose file or directory cannot be made durable is not committed —
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

    /// An unreadable directory is an error for `clear`, not an empty success.
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
