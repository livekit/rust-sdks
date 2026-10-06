//! One pipeline per process, the way `log` and `tracing` do it: install it once (or again),
//! reach it anywhere without a handle, and every call is a no-op while none is installed. The
//! core does not assume an async runtime, so whoever builds the pipeline spawns its exporter and
//! shuts down the one `install` hands back.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, RwLock, Weak,
    },
    time::Duration,
};

use tokio::{
    sync::{mpsc::WeakUnboundedSender, oneshot},
    time::timeout,
};

use crate::{
    exporter::Command, telemetry::Shared, AttributeValue, DeviceEvent, DeviceState, LogRecord,
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
    fn start(&self);
    fn stop(&self);
}

struct Installed {
    telemetry: Telemetry,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
}

static SHARED: RwLock<Option<Installed>> = RwLock::new(None);

/// Serializes the lifecycle — install (with its instruments' `start`), uninstall and disable
/// (with their `stop`) — so an instrument can never start after the opt-out stopped it. Held
/// while instruments run: they may call back into the capture functions (which only read
/// `SHARED`), but must not install or disable from `start`/`stop` — nor block waiting on a thread
/// or queue that may itself be inside `install`/`disable` (e.g. a synchronous hop to a platform
/// queue that is configuring telemetry): that would deadlock. Hop asynchronously instead.
static LIFECYCLE: Mutex<()> = Mutex::new(());

/// The app opted out ([`disable`]): nothing installs for the rest of the process. Written and
/// read under `LIFECYCLE`.
static DISABLED: AtomicBool = AtomicBool::new(false);

/// Every pipeline generation ever installed that may still run — the current one and replaced
/// ones still draining — reachable without being kept alive, so the opt-out can revoke, cancel
/// and await each.
struct Generation {
    shared: Weak<Shared>,
    commands: WeakUnboundedSender<Command>,
}

static GENERATIONS: Mutex<Vec<Generation>> = Mutex::new(Vec::new());

fn lifecycle() -> MutexGuard<'static, ()> {
    LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner())
}

fn current() -> Option<Telemetry> {
    SHARED.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|i| i.telemetry.clone())
}

/// Make `telemetry` the process pipeline and start `instruments` on it. Returns the pipeline it
/// replaces (its instruments already stopped), still holding data: the caller shuts it down.
/// After [`disable`] nothing installs: `telemetry` is revoked, purged and handed back instead.
pub fn install(
    telemetry: Telemetry,
    instruments: Vec<Arc<dyn TelemetryInstrument>>,
) -> Option<Telemetry> {
    let _lifecycle = lifecycle();
    if is_disabled() {
        telemetry.shared.revoked.store(true, Ordering::SeqCst);
        telemetry.shared.clear();
        return Some(telemetry);
    }
    {
        let mut generations = GENERATIONS.lock().unwrap_or_else(|e| e.into_inner());
        generations.retain(|g| g.shared.strong_count() > 0);
        generations.push(Generation {
            shared: Arc::downgrade(&telemetry.shared),
            commands: telemetry.weak_commands(),
        });
    }
    // `SHARED` is released before any instrument runs: `start` may call straight back in.
    let previous = SHARED
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Installed { telemetry, instruments: instruments.clone() });
    if let Some(previous) = &previous {
        for instrument in &previous.instruments {
            instrument.stop();
        }
    }
    for instrument in &instruments {
        instrument.start();
    }
    previous.map(|p| p.telemetry)
}

/// Stop the instruments and remove the process pipeline; the caller shuts it down.
pub fn uninstall() -> Option<Telemetry> {
    let _lifecycle = lifecycle();
    let taken = SHARED.write().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(taken) = &taken {
        for instrument in &taken.instruments {
            instrument.stop();
        }
    }
    taken.map(|i| i.telemetry)
}

/// Opt-out: stop collecting for the rest of the process. Before this returns, the pipeline is
/// removed, its instruments stopped and every generation — the installed one and replaced ones
/// still draining — revoked: [`scope`] is `None`, later [`install`]s are refused and every
/// capture, cache write and upload refuses from here on. The returned future does the rest —
/// cancel and await each exporter, delete everything it held (queue, spans, RTC windows, every
/// cached batch on disk) without another upload — and resolves `false` when some cached data
/// could not be deleted. Spawn it; dropping it leaves the data in place until a later configure
/// purges it.
pub fn disable() -> impl std::future::Future<Output = bool> + Send + 'static {
    let generations: Vec<(Arc<Shared>, WeakUnboundedSender<Command>)> = {
        let _lifecycle = lifecycle();
        DISABLED.store(true, Ordering::SeqCst);
        let taken = SHARED.write().unwrap_or_else(|e| e.into_inner()).take();
        for instrument in taken.iter().flat_map(|t| &t.instruments) {
            instrument.stop();
        }
        let generations: Vec<Generation> =
            GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect();
        generations
            .into_iter()
            .filter_map(|g| Some((g.shared.upgrade()?, g.commands)))
            .inspect(|(shared, _)| {
                // Every capture and cache write checks this under the lock it commits under, so
                // nothing lands after the clear below.
                shared.revoked.store(true, Ordering::SeqCst);
            })
            .collect()
    };
    async move {
        let mut complete = true;
        for (shared, commands) in generations {
            complete &= shared.clear();
            // Cancel the exporter's request in flight (a pulled request is then never handed
            // out) and wait until it has stopped touching storage.
            if let Some(commands) = commands.upgrade() {
                let (done, stopped) = oneshot::channel();
                if commands.send(Command::Purge(done)).is_ok() {
                    let bound = Duration::from_millis(shared.config.export_timeout_ms.max(1));
                    let _ = timeout(bound + Duration::from_secs(1), stopped).await;
                }
            }
            // Whatever it wrote while winding down.
            complete &= shared.clear();
        }
        complete
    }
}

/// Forget the opt-out and every generation (tests only: the switch is one-way in a process).
#[cfg(test)]
pub(crate) fn reset_for_test() {
    let _lifecycle = lifecycle();
    DISABLED.store(false, Ordering::SeqCst);
    SHARED.write().unwrap_or_else(|e| e.into_inner()).take();
    GENERATIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Serializes tests that use the process-wide pipeline.
#[cfg(test)]
pub(crate) static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Whether the app opted out with [`disable`].
pub fn is_disabled() -> bool {
    DISABLED.load(Ordering::SeqCst)
}

/// The installed pipeline, if any.
pub fn shared() -> Option<Telemetry> {
    current()
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
