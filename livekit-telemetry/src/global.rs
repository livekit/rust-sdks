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

//! One pipeline per process, the way `log` and `tracing` do it: install it once (or again),
//! reach it anywhere without a handle, and every call is a no-op while none is installed. The
//! core does not assume an async runtime, so whoever builds the pipeline spawns its exporter and
//! shuts down the one `install` hands back.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, RwLock, Weak,
    },
};

use crate::{
    telemetry::Shared, AttributeValue, BatchCache, DeviceEvent, DeviceState, FileCache, LogRecord,
    Scope, Telemetry, TelemetryEvent, TelemetryStats,
};

/// A platform instrument — device signals, log capture — that feeds the pipeline while it runs.
/// Started right after the pipeline is installed and stopped when it goes; calls may come from
/// any thread and must not block.
///
/// `start` and `stop` run synchronously under the lifecycle lock on the thread performing the
/// lifecycle operation (the opt-out's `stop` inside [`disable`], before it returns). They must
/// not call [`install`] or [`disable`], poll [`shutdown`], or synchronously wait for telemetry
/// lifecycle work, including work on another thread or queue: that would re-enter the same
/// non-reentrant lock. Constructing the `shutdown()` future is safe, as is scheduling it without
/// waiting for it.
///
/// The same restrictions apply to instrument destructors, which may also run under the lifecycle
/// lock. Capture calls, including logging, are supported from hooks and destructors.
#[cfg_attr(feature = "uniffi", uniffi::export(with_foreign))]
pub trait TelemetryInstrument: Send + Sync {
    /// Runs once, at install.
    fn start(&self);
    /// Runs once, at replace, uninstall or opt-out, while its pipeline is still the installed one
    /// (except at the opt-out, after which a last record goes nowhere).
    fn stop(&self);
}

struct Installed {
    telemetry: Telemetry,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
}

/// The installed process pipeline and its instruments.
static INSTALLED: RwLock<Option<Installed>> = RwLock::new(None);

/// Serializes the lifecycle — install (with its instruments' `start`), uninstall and disable
/// (with their `stop`) — so an instrument can never start after the opt-out stopped it. Held
/// while instruments run: they may call back into the capture functions (which only read
/// `INSTALLED`), but hooks and destructors must not re-enter lifecycle operations or poll
/// `shutdown`, nor wait on a thread or queue performing lifecycle work. Schedule it without
/// waiting instead; see [`TelemetryInstrument`].
static LIFECYCLE: Mutex<()> = Mutex::new(());

/// The app opted out ([`disable`]): nothing installs for the rest of the process. Written
/// under `LIFECYCLE`, so an install cannot race the opt-out; read anywhere without it
/// ([`is_disabled`] is a plain atomic load, and so is the check at the top of `install`, which
/// then holds the lock).
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Live pipelines and deduplicated disk paths, retained independently so retired weak
/// allocations can be freed even when a newer pipeline still uses the same directory.
/// Pending paths leave `stored` only after a successful opt-out erase.
#[derive(Default)]
struct Generations {
    live: Vec<Weak<Shared>>,
    stored: BTreeSet<PathBuf>,
}

impl Generations {
    fn compact(&mut self) {
        self.live.retain(|shared| shared.strong_count() > 0);
    }

    /// Make `shared` known to later opt-outs, once however often its pipeline is installed.
    fn register(&mut self, shared: &Arc<Shared>) {
        self.compact();
        let shared = Arc::downgrade(shared);
        if !self.live.iter().any(|known| known.ptr_eq(&shared)) {
            self.live.push(shared);
        }
    }
}

/// Erase `dir`, the storage directory of a live or gone generation: open it as a [`FileCache`],
/// which removes the files that are not batches, and clear it. Logs and returns `false` when
/// something stays, or when `dir` could not be inspected.
fn erase(dir: &Path) -> bool {
    let erased = match dir.try_exists() {
        Ok(false) => Ok(()),
        Ok(true) => FileCache::open(dir, u64::MAX).and_then(|cache| cache.clear()),
        Err(err) => Err(err),
    };
    if let Err(err) = &erased {
        log::warn!("opt-out: cached telemetry could not all be deleted ({err})");
    }
    erased.is_ok()
}

static GENERATIONS: Mutex<Generations> =
    Mutex::new(Generations { live: Vec::new(), stored: BTreeSet::new() });

fn lifecycle() -> MutexGuard<'static, ()> {
    LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The installed pipeline, if any.
pub(crate) fn current() -> Option<Telemetry> {
    INSTALLED.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|i| i.telemetry.clone())
}

/// Stop the installed pipeline's instruments while it is still the installed one, so a last
/// record from `stop` reaches it. `INSTALLED` is not held while an instrument runs: `stop` may call
/// straight back in.
fn stop_installed_instruments() {
    let instruments = INSTALLED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|i| i.instruments.clone())
        .unwrap_or_default();
    for instrument in &instruments {
        instrument.stop();
    }
}

/// Make `telemetry` the process pipeline and start `instruments` on it. Returns the pipeline it
/// replaces (its instruments already stopped), still holding data: the caller shuts it down.
/// Installing the installed pipeline again replaces only its instruments and returns `None`:
/// there is no outgoing pipeline to shut down.
///
/// Delivery is at least once across a replacement, as across a restart: a batch the replaced
/// pipeline uploaded but could not delete from its storage directory is uploaded once more by
/// the next pipeline on that directory.
///
/// After [`disable`] nothing installs: `telemetry` is revoked, purged, cancelled and handed back
/// instead. It stays known to later [`disable`] calls while the caller holds it, so they retry
/// a delete that failed here. Its exporter is still the caller's: one already spawned stops by
/// itself and the caller joins its task, and one never spawned is dropped by the caller (nothing
/// here owns it).
pub fn install(
    telemetry: Telemetry,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> Option<Telemetry> {
    let lifecycle = lifecycle();
    if is_disabled() {
        telemetry.shared.revoke();
        GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()).register(&telemetry.shared);
        // Nothing else stops an exporter spawned for it: without this it ticks for the rest of
        // the process.
        telemetry.shared.cancel.notify_one();
        // Deleted outside the lock: only this caller holds the refused pipeline. `clear` logs an
        // incomplete delete itself.
        drop(lifecycle);
        telemetry.shared.clear();
        erase_refused_storage(telemetry.shared.config.storage_dir.as_deref());
        return Some(telemetry);
    }
    {
        let mut generations = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        generations.register(&telemetry.shared);
        if let Some(dir) = &telemetry.shared.config.storage_dir {
            generations.stored.insert(PathBuf::from(dir));
        }
    }
    stop_installed_instruments();
    let shared = telemetry.shared.clone();
    // `INSTALLED` is released before any instrument runs: `start` may call straight back in.
    let previous = INSTALLED
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Installed { telemetry, instruments: instruments.clone() });
    for instrument in &instruments {
        instrument.start();
    }
    // The installed pipeline installed again is not an outgoing one.
    previous.map(|p| p.telemetry).filter(|p| !Arc::ptr_eq(&p.shared, &shared))
}

/// Erase refused storage and retain a failed path for the next opt-out.
fn erase_refused_storage(storage_dir: Option<&str>) {
    if let Some(dir) = storage_dir {
        if !erase(Path::new(dir)) {
            GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()).stored.insert(PathBuf::from(dir));
        }
    }
}

/// Handle configuration refusal without constructing a pipeline.
///
/// Returns whether opt-out refuses configuration, not whether erasure completed. Failed
/// erasure stays registered for the next opt-out. This bridge helper keeps admission outside
/// cache construction and uses the same configured-directory policy as [`install`].
#[doc(hidden)]
pub fn refuse_configuration(storage_dir: Option<&str>) -> bool {
    let lifecycle = lifecycle();
    if !is_disabled() {
        return false;
    }
    drop(lifecycle);
    erase_refused_storage(storage_dir);
    true
}

/// Stop the instruments and remove the process pipeline; the caller shuts it down. With
/// [`install`], the synchronous half of [`shutdown`]: a caller that orders installs and uninstalls
/// itself drains the pipeline it gets back when it is ready to.
pub fn uninstall() -> Option<Telemetry> {
    let _lifecycle = lifecycle();
    uninstall_locked()
}

/// Remove only this pipeline after a bridge delivery failure.
///
/// A later configuration stays installed. This bridge helper does not erase cached data.
#[doc(hidden)]
pub fn uninstall_if_current(telemetry: &Telemetry) -> Option<Telemetry> {
    let _lifecycle = lifecycle();
    let matches = INSTALLED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|installed| Arc::ptr_eq(&installed.telemetry.shared, &telemetry.shared));
    if !matches {
        return None;
    }
    uninstall_locked()
}

/// Remove the current pipeline while the caller holds the lifecycle lock.
fn uninstall_locked() -> Option<Telemetry> {
    stop_installed_instruments();
    // A statement of its own, so the write guard ends here: the instruments freed below may log
    // or read the pipeline from their destructors.
    let installed = INSTALLED.write().unwrap_or_else(|e| e.into_inner()).take();
    installed.map(|i| i.telemetry)
}

/// Opt-out: stop collecting for the rest of the process. Before this returns, the pipeline is
/// removed, its instruments stopped and every generation — the installed one and replaced ones
/// still draining — revoked: [`scope`] is `None`, later [`install`]s are refused and every
/// capture, cache write and upload refuses from here on. The returned future does the rest —
/// cancel and await each exporter, running or draining, delete everything it held (queue, spans,
/// RTC windows, every cached batch on disk) without another upload, then delete what each live or
/// gone generation left in its storage directory — and resolves `false` when some cached data
/// could not be deleted. Spawn it; dropping it leaves the cleanup incomplete until a later
/// `disable()` future is awaited, or until a later configure names the same storage directory
/// (a refused configure erases only its own). Every call revokes, purges and awaits each generation still alive, so a repeated
/// call resolves only once the cache is deleted, even while an earlier one is still running.
/// All exporters are cancelled before waiting; their aggregate wait is at most the largest
/// `export_timeout_ms` (1 s minimum) plus 1 s. Synchronous cache and filesystem work adds time
/// outside that wait bound. A failed final per-cache clear keeps the result `false` even if
/// the later directory sweep succeeds; that sweep cannot certify a caller-provided cache.
pub fn disable() -> impl std::future::Future<Output = bool> + Send + 'static {
    let (live, stored) = {
        let _lifecycle = lifecycle();
        DISABLED.store(true, Ordering::SeqCst);
        let taken = INSTALLED.write().unwrap_or_else(|e| e.into_inner()).take();
        for instrument in taken.iter().flat_map(|t| &t.instruments) {
            instrument.stop();
        }
        // A snapshot, not a drain: a second call must find (and purge) them too.
        let mut list = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        list.compact();
        // Every pipeline still alive is held before the installed handle goes, so the opt-out
        // reaches its exporter even when that handle was the last one. Revoked here: every
        // capture, cache write and request checks it under the lock it commits under, so
        // nothing lands or starts after this returns.
        let live: Vec<Arc<Shared>> =
            list.live.iter().filter_map(Weak::upgrade).inspect(|shared| shared.revoke()).collect();
        // Every generation's disk, deleted after the live purges, which count what they delete.
        // A live pipeline that fell back to memory at start leaves an earlier launch's batches
        // there, and a gone one leaves what it cached.
        let stored: Vec<PathBuf> = list.stored.iter().cloned().collect();
        // Freed outside every lock but the lifecycle one: an instrument's destructor may log.
        drop(list);
        drop(taken);
        (live, stored)
    };
    async move {
        let mut complete = true;
        let wait = live
            .iter()
            .map(|shared| shared.config.export_timeout() + crate::telemetry::SHUTDOWN_GRACE)
            .max()
            .unwrap_or_default();
        for shared in &live {
            shared.cancel.notify_one();
        }
        for shared in &live {
            shared.clear();
        }
        let deadline = tokio::time::Instant::now() + wait;
        for shared in live {
            let mut alive = shared.exporter_alive.clone();
            let _ = tokio::time::timeout_at(deadline, alive.changed()).await;
            complete &= shared.clear();
        }
        for dir in stored {
            let erased = erase(&dir);
            complete &= erased;
            if erased {
                let mut list = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
                list.stored.remove(&dir);
                list.compact();
            }
        }
        complete
    }
}

/// Forget the opt-out and every generation (tests only: the switch is one-way in a process).
#[cfg(test)]
pub(crate) fn reset_for_test() {
    let _lifecycle = lifecycle();
    DISABLED.store(false, Ordering::SeqCst);
    let taken = INSTALLED.write().unwrap_or_else(|e| e.into_inner()).take();
    drop(taken);
    *GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()) = Generations::default();
}

/// Serializes tests that use the process-wide pipeline.
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whether the app opted out with [`disable`].
pub fn is_disabled() -> bool {
    DISABLED.load(Ordering::SeqCst)
}

/// A new scope (one room, one call) on the process pipeline; `None` while telemetry is off.
pub fn scope() -> Option<Scope> {
    current().map(|t| t.begin_scope())
}

/// [`Telemetry::emit`] on the installed pipeline.
pub fn emit(event: TelemetryEvent) {
    if let Some(t) = current() {
        t.emit(event);
    }
}

/// [`Telemetry::log`] on the installed pipeline.
pub fn log(record: LogRecord) {
    if let Some(t) = current() {
        t.log(record);
    }
}

/// [`log()`] with the record built only when a pipeline is installed at the call. The installed
/// pipeline is read once, as a snapshot: when there is none, `record` never runs and the call
/// costs one read-lock acquisition. When there is one, `record` runs after the read and its record
/// goes to that pipeline: a call that overlaps an uninstall or a replace can still build its
/// record after the uninstall returned. That record is exported only if the pipeline it read still
/// exports, is refused once the opt-out revoked that pipeline, and never goes to a replacing one.
pub fn log_with(record: impl FnOnce() -> Option<LogRecord>) {
    if let Some(t) = current() {
        if let Some(record) = record() {
            t.log(record);
        }
    }
}

/// [`Telemetry::device_event`] on the installed pipeline.
pub fn device_event(event: DeviceEvent) {
    if let Some(t) = current() {
        t.device_event(event);
    }
}

/// [`Telemetry::set_device_state`] on the installed pipeline.
pub fn set_device_state(state: DeviceState) {
    if let Some(t) = current() {
        t.set_device_state(state);
    }
}

/// [`Telemetry::set_attribute`] (SDK metadata) on the installed pipeline.
pub fn set_attribute(key: &str, value: Option<AttributeValue>) {
    if let Some(t) = current() {
        t.set_attribute(key, value);
    }
}

/// [`Telemetry::stats`] of the installed pipeline.
pub fn stats() -> Option<TelemetryStats> {
    current().map(|t| t.stats())
}

/// The one-line health readout, or `off`.
pub fn diagnostics() -> String {
    stats().map_or_else(|| "off".to_owned(), |s| s.to_string())
}

/// [`Telemetry::flush`] on the installed pipeline.
pub async fn flush() {
    if let Some(t) = current() {
        t.flush().await;
    }
}

/// Uninstall and drain: the last flush, then the pipeline stops.
pub async fn shutdown() {
    if let Some(t) = uninstall() {
        t.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::tests::{test_config, FakeTransport};
    use std::{fs, future::Future};

    #[tokio::test(start_paused = true)]
    async fn exporter_waits_share_one_timeout_budget() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        let mut exporters = Vec::new();
        let mut shared = Vec::new();
        for _ in 0..3 {
            let config = crate::TelemetryConfig { export_timeout_ms: 1000, ..test_config() };
            let (telemetry, exporter) = Telemetry::new(config, FakeTransport::scripted([]));
            shared.push(telemetry.shared.clone());
            exporters.push(exporter); // Held, deliberately never spawned.
            drop(install(telemetry, Vec::new()));
        }
        let purge = disable();
        let started = tokio::time::Instant::now();
        // Poll once: every exporter must already have received its cancellation signal.
        tokio::pin!(purge);
        std::future::poll_fn(|cx| {
            assert!(purge.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        // Consuming the cancellation permits is intentional: these exporters never run.
        let all_cancelled = shared.iter().all(|s| {
            let notified = s.cancel.notified();
            tokio::pin!(notified);
            std::future::Future::poll(
                notified.as_mut(),
                &mut std::task::Context::from_waker(std::task::Waker::noop()),
            )
            .is_ready()
        });
        assert!(purge.await);
        let elapsed = started.elapsed();
        drop(exporters);
        reset_for_test();
        assert!(
            all_cancelled,
            "later generations were not cancelled before waiting; elapsed {elapsed:?}"
        );
        assert!(elapsed <= std::time::Duration::from_secs(2), "waits accumulated: {elapsed:?}");
    }

    #[tokio::test]
    async fn retired_generations_do_not_accumulate_for_one_directory() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        let dir = crate::cache::temp_dir("compact-generations");
        let cache = FileCache::open(&dir, u64::MAX).unwrap();
        cache.push("00000000000000000001-000001-1-l", b"cached").unwrap();
        drop(cache);
        for _ in 0..64 {
            let config = crate::TelemetryConfig {
                storage_dir: Some(dir.to_string_lossy().into_owned()),
                ..test_config()
            };
            let (telemetry, exporter) = Telemetry::new(config, FakeTransport::scripted([]));
            drop(exporter);
            drop(install(telemetry, Vec::new()));
            drop(uninstall());
        }
        let (count, directories) = {
            let list = GENERATIONS.lock().unwrap();
            (list.live.len(), list.stored.len())
        };
        assert!(disable().await);
        reset_for_test();
        fs::remove_dir_all(dir).unwrap();
        assert_eq!(directories, 1);
        assert!(count <= 1, "retained {count} dead weak allocations for one directory");
    }

    #[tokio::test]
    async fn synchronous_registry_keeps_uninspected_directories() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        let dir = crate::cache::temp_dir("uninspected-registry");
        fs::create_dir(&dir).unwrap();
        GENERATIONS.lock().unwrap().stored.insert(dir.clone());
        let purge = disable();
        // Even an empty directory stays pending until the asynchronous cleanup phase.
        let retained = GENERATIONS.lock().unwrap().stored.contains(&dir);
        assert!(purge.await);
        reset_for_test();
        fs::remove_dir_all(dir).unwrap();
        assert!(retained, "the synchronous phase inspected and discarded the directory");
    }

    #[tokio::test]
    async fn refused_erase_is_retried_by_later_opt_out() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        assert!(disable().await);
        let root = crate::cache::temp_dir("refused-retry");
        let backup = root.with_extension("backup");
        fs::create_dir(&root).unwrap();
        let dir = root.join("cache");
        let cache = FileCache::open(&dir, u64::MAX).unwrap();
        cache.push("00000000000000000001-000001-1-l", b"cached").unwrap();
        drop(cache);
        fs::rename(&root, &backup).unwrap();
        fs::write(&root, b"temporarily inaccessible parent").unwrap();
        let config = crate::TelemetryConfig {
            storage_dir: Some(dir.to_string_lossy().into_owned()),
            ..test_config()
        };
        let (telemetry, exporter) = Telemetry::new(config, FakeTransport::scripted([]));
        drop(exporter);
        drop(install(telemetry, Vec::new()));
        fs::remove_file(&root).unwrap();
        fs::rename(&backup, &root).unwrap();
        assert!(disable().await);
        let left = fs::read_dir(&dir).unwrap().count();
        reset_for_test();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(left, 0, "a later opt-out must retry the refused configuration");
    }

    struct CaptureOnDrop;

    impl TelemetryInstrument for CaptureOnDrop {
        fn start(&self) {}
        fn stop(&self) {}
    }

    impl Drop for CaptureOnDrop {
        fn drop(&mut self) {
            assert!(GENERATIONS.try_lock().is_ok(), "generation registry held during Drop");
            emit(TelemetryEvent::new("custom.instrument_dropped"));
        }
    }

    #[tokio::test]
    async fn instrument_destruction_can_capture() {
        let _global = TEST_LOCK.lock().await;
        for path in ["uninstall", "disable", "reset"] {
            reset_for_test();
            let (telemetry, exporter) = Telemetry::new(test_config(), FakeTransport::scripted([]));
            drop(exporter);
            install(telemetry, vec![Arc::new(CaptureOnDrop)]);
            match path {
                "uninstall" => drop(uninstall()),
                "disable" => assert!(disable().await),
                _ => reset_for_test(),
            }
        }
        reset_for_test();
    }

    /// A caller-provided cache whose `clear` fails while it is locked.
    struct Lockable {
        inner: crate::MemoryCache,
        locked: AtomicBool,
    }

    impl BatchCache for Lockable {
        fn push(&self, id: &str, body: &[u8]) -> std::io::Result<Vec<String>> {
            self.inner.push(id, body)
        }
        fn replace(&self, old: &str, new: &[(String, Vec<u8>)]) -> std::io::Result<Vec<String>> {
            self.inner.replace(old, new)
        }
        fn pending(&self) -> Vec<String> {
            self.inner.pending()
        }
        fn read(&self, id: &str) -> std::io::Result<Vec<u8>> {
            self.inner.read(id)
        }
        fn remove(&self, id: &str) -> std::io::Result<()> {
            self.inner.remove(id)
        }
        fn clear(&self) -> std::io::Result<()> {
            if self.locked.load(Ordering::SeqCst) {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            }
            self.inner.clear()
        }
    }

    /// A pipeline refused after the opt-out stays known to it: its spawned exporter is cancelled,
    /// and a later `disable` deletes what its caller-provided cache could not delete at the
    /// refusal. The caller still owns the exporter task and the pipeline.
    #[tokio::test(start_paused = true)]
    async fn a_refused_pipeline_is_cancelled_and_purged_by_a_later_opt_out() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        assert!(disable().await);
        let cache = Arc::new(Lockable {
            inner: crate::MemoryCache::new(1 << 20),
            locked: AtomicBool::new(true),
        });
        cache.inner.push("00000000000000000001-000001-1-l", b"cached").unwrap();
        let (telemetry, exporter) =
            Telemetry::with_cache(test_config(), FakeTransport::scripted([]), cache.clone());
        let mut task = tokio::spawn(exporter.run());
        let refused = install(telemetry, Vec::new()).expect("refused pipelines are handed back");
        let stopped =
            tokio::time::timeout(std::time::Duration::from_secs(5), &mut task).await.is_ok();
        let kept = cache.inner.pending().len();
        cache.locked.store(false, Ordering::SeqCst);
        let complete = disable().await;
        let left = cache.inner.pending().len();
        let shared = Arc::downgrade(&refused.shared);
        drop(refused);
        if !stopped {
            task.abort();
            let _ = task.await;
        }
        reset_for_test();
        assert!(stopped, "the refused exporter kept running");
        assert_eq!((kept, complete, left), (1, true, 0), "a later opt-out retries the delete");
        assert!(shared.upgrade().is_none(), "the refused pipeline is released");
    }

    /// Installing the installed pipeline again replaces nothing: no pipeline is handed back for
    /// the caller to shut down, and it stays one generation.
    #[tokio::test]
    async fn reinstalling_the_installed_pipeline_hands_nothing_back() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        let (telemetry, exporter) = Telemetry::new(test_config(), FakeTransport::scripted([]));
        drop(exporter);
        assert!(install(telemetry.clone(), Vec::new()).is_none());
        let handed_back =
            (0..64).filter(|_| install(telemetry.clone(), Vec::new()).is_some()).count();
        let registered = GENERATIONS.lock().unwrap().live.len();
        let installed = current().is_some_and(|t| Arc::ptr_eq(&t.shared, &telemetry.shared));
        assert!(disable().await);
        reset_for_test();
        assert_eq!((handed_back, registered, installed), (0, 1, true));
    }
}
