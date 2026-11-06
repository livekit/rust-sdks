//! One pipeline per process, the way `log` and `tracing` do it: install it once (or again),
//! reach it anywhere without a handle, and every call is a no-op while none is installed. The
//! core does not assume an async runtime, so whoever builds the pipeline spawns its exporter and
//! shuts down the one `install` hands back.

use std::{
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
/// `start` and `stop` run synchronously on the thread that installs or opts out, under the
/// lifecycle lock (the opt-out's `stop` inside [`disable`], before it returns): they must not
/// block on that caller's queue and must not install, disable or wait on telemetry from inside
/// (capture calls are fine).
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
/// `INSTALLED`), but must not install or disable from `start`/`stop` — nor block waiting on a thread
/// or queue that may itself be inside `install`/`disable` (e.g. a synchronous hop to a platform
/// queue that is configuring telemetry): that would deadlock. Hop asynchronously instead.
static LIFECYCLE: Mutex<()> = Mutex::new(());

/// The app opted out ([`disable`]): nothing installs for the rest of the process. Written and
/// read under `LIFECYCLE`.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Every pipeline generation ever installed that may still hold data — the current one,
/// replaced ones still draining, and the disk cache of any that are gone — reachable without
/// being kept alive, so the opt-out can revoke, cancel and await each, and delete what each left
/// on disk.
struct Generation {
    shared: Weak<Shared>,
    /// The `storage_dir` it was built with (none for a caller-provided cache), kept until it is
    /// known to hold nothing.
    storage: Option<PathBuf>,
}

impl Generation {
    /// Keep disk paths until the asynchronous opt-out verifies their erasure. Never inspect
    /// storage while holding the lifecycle and generation registries.
    fn holds_data(&self) -> bool {
        self.shared.strong_count() > 0 || self.storage.is_some()
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

static GENERATIONS: Mutex<Vec<Generation>> = Mutex::new(Vec::new());

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
/// After [`disable`] nothing installs: `telemetry` is revoked, purged and handed back instead.
pub fn install(
    telemetry: Telemetry,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> Option<Telemetry> {
    let lifecycle = lifecycle();
    if is_disabled() {
        telemetry.shared.revoke();
        // Deleted outside the lock: only this caller holds the refused pipeline. `clear` logs an
        // incomplete delete itself.
        drop(lifecycle);
        telemetry.shared.clear();
        // A storage dir it could not open still holds what an earlier launch cached.
        if let Some(dir) = telemetry.shared.config.storage_dir.as_deref() {
            if !erase(Path::new(dir)) {
                GENERATIONS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Generation { shared: Weak::new(), storage: Some(PathBuf::from(dir)) });
            }
        }
        return Some(telemetry);
    }
    {
        let mut generations = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        generations.retain(Generation::holds_data);
        generations.push(Generation {
            shared: Arc::downgrade(&telemetry.shared),
            storage: telemetry.shared.config.storage_dir.as_ref().map(PathBuf::from),
        });
    }
    stop_installed_instruments();
    // `INSTALLED` is released before any instrument runs: `start` may call straight back in.
    let previous = INSTALLED
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Installed { telemetry, instruments: instruments.clone() });
    for instrument in &instruments {
        instrument.start();
    }
    previous.map(|p| p.telemetry)
}

/// Stop the instruments and remove the process pipeline; the caller shuts it down. With
/// [`install`], the synchronous half of [`shutdown`]: a caller that orders installs and uninstalls
/// itself drains the pipeline it gets back when it is ready to.
pub fn uninstall() -> Option<Telemetry> {
    let _lifecycle = lifecycle();
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
/// could not be deleted. Spawn it; dropping it leaves the data in place until a later configure
/// purges it. Every call revokes, purges and awaits each generation still alive, so a repeated
/// call resolves only once the cache is deleted, even while an earlier one is still running.
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
        list.retain(Generation::holds_data);
        // Every pipeline still alive is held before the installed handle goes, so the opt-out
        // reaches its exporter even when that handle was the last one. Revoked here: every
        // capture, cache write and request checks it under the lock it commits under, so
        // nothing lands or starts after this returns.
        let live: Vec<Arc<Shared>> = list
            .iter()
            .filter_map(|g| g.shared.upgrade())
            .inspect(|shared| shared.revoke())
            .collect();
        // Every generation's disk, deleted after the live purges, which count what they delete.
        // A live pipeline that fell back to memory at start leaves an earlier launch's batches
        // there, and a gone one leaves what it cached.
        let mut stored: Vec<PathBuf> = list.iter().filter_map(|g| g.storage.clone()).collect();
        stored.sort_unstable();
        stored.dedup();
        // Freed outside every lock but the lifecycle one: an instrument's destructor may log.
        drop(list);
        drop(taken);
        (live, stored)
    };
    async move {
        let mut complete = true;
        for shared in live {
            // Cancels the exporter, running or draining, and waits until it stopped.
            complete &= shared.purge().await;
        }
        for dir in stored {
            let erased = erase(&dir);
            complete &= erased;
            if erased {
                let mut list = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
                for generation in list.iter_mut() {
                    if generation.storage.as_ref() == Some(&dir) {
                        generation.storage = None;
                    }
                }
                list.retain(Generation::holds_data);
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
    let installed = INSTALLED.write().unwrap_or_else(|e| e.into_inner()).take();
    drop(installed);
    GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
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
    use std::fs;

    #[tokio::test]
    async fn synchronous_registry_keeps_uninspected_directories() {
        let _global = TEST_LOCK.lock().await;
        reset_for_test();
        let dir = crate::cache::temp_dir("uninspected-registry");
        fs::create_dir(&dir).unwrap();
        GENERATIONS
            .lock()
            .unwrap()
            .push(Generation { shared: Weak::new(), storage: Some(dir.clone()) });
        let purge = disable();
        // Even an empty directory stays pending until the asynchronous cleanup phase.
        let retained = GENERATIONS.lock().unwrap().iter().any(|g| g.storage.as_ref() == Some(&dir));
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
}
