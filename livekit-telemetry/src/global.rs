//! One pipeline per process, the way `log` and `tracing` do it: install it once (or again),
//! reach it anywhere without a handle, and every call is a no-op while none is installed. The
//! core does not assume an async runtime, so whoever builds the pipeline spawns its exporter and
//! shuts down the one `install` hands back.

use std::sync::RwLock;

use crate::{
    AttributeValue, DeviceEvent, DeviceState, LogRecord, Scope, Telemetry, TelemetryEvent,
    TelemetryStats,
};

static SHARED: RwLock<Option<Telemetry>> = RwLock::new(None);

fn current() -> Option<Telemetry> {
    SHARED.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Make `telemetry` the process pipeline. Returns the one it replaces, still holding data: the
/// caller shuts it down.
pub fn install(telemetry: Telemetry) -> Option<Telemetry> {
    SHARED.write().unwrap_or_else(|e| e.into_inner()).replace(telemetry)
}

/// Remove the process pipeline; the caller shuts it down.
pub fn uninstall() -> Option<Telemetry> {
    SHARED.write().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn shared() -> Option<Telemetry> {
    current()
}

/// A new scope (one room, one call) on the process pipeline; `None` while telemetry is off.
pub fn scope() -> Option<Scope> {
    current().map(|t| t.begin_scope())
}

pub fn emit(event: TelemetryEvent) {
    if let Some(t) = current() {
        t.emit(event);
    }
}

pub fn log(record: LogRecord) {
    if let Some(t) = current() {
        t.log(record);
    }
}

pub fn device_event(event: DeviceEvent) {
    if let Some(t) = current() {
        t.device_event(event);
    }
}

pub fn set_device_state(state: DeviceState) {
    if let Some(t) = current() {
        t.set_device_state(state);
    }
}

pub fn set_server(url: &str, token: &str) {
    if let Some(t) = current() {
        t.set_server(url, token);
    }
}

pub fn set_attribute(key: &str, value: Option<AttributeValue>) {
    if let Some(t) = current() {
        t.set_attribute(key, value);
    }
}

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
