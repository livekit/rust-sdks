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

use std::{collections::HashMap, sync::Arc, time::Duration};

use livekit_runtime::timeout;
use tokio::sync::{mpsc, oneshot};

use crate::{
    event::now_unix_nanos, exporter::Command, persist::FileCache, store::Store, Attribute,
    Exporter, TelemetryEvent, TelemetryTransport,
};

/// Pipeline configuration.
///
/// Defaults follow OTel's `BatchLogRecordProcessor` (1 s delay, 2048 queue, 512 batch);
/// production tuning (e.g. a 15 s cadence) is a config change, not a code change.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Full OTLP/HTTP logs URL: `http://localhost:4318/v1/logs` locally,
    /// `https://<domain>/observability/logs/otlp/v0` for LiveKit Cloud.
    pub endpoint: String,
    /// Extra request headers, e.g. `Authorization: Bearer <token>`.
    pub headers: HashMap<String, String>,
    /// Resource attributes describing the emitter (`service.name`, `os.name`,
    /// `device.model.identifier`, `session.id`, …). `telemetry.sdk.*` are filled in by the core.
    pub resource: Vec<Attribute>,
    /// Directory for the on-disk cache of undeliverable batches (created if missing). `None`
    /// disables persistence: batches that fail after retries are dropped.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub storage_dir: Option<String>,
    /// Cap on the on-disk cache; the oldest batches are evicted first.
    #[cfg_attr(feature = "uniffi", uniffi(default = 4194304))]
    pub max_storage_bytes: u64,
    #[cfg_attr(feature = "uniffi", uniffi(default = 1000))]
    pub flush_interval_ms: u64,
    /// Events buffered before the oldest are dropped.
    #[cfg_attr(feature = "uniffi", uniffi(default = 2048))]
    pub max_queue_size: u32,
    /// Events per export request.
    #[cfg_attr(feature = "uniffi", uniffi(default = 512))]
    pub max_batch_size: u32,
    /// Bound on a single transport attempt, and on `shutdown`.
    #[cfg_attr(feature = "uniffi", uniffi(default = 10000))]
    pub export_timeout_ms: u64,
}

impl TelemetryConfig {
    /// Defaults for the given endpoint; no persistence.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            headers: HashMap::new(),
            resource: Vec::new(),
            storage_dir: None,
            max_storage_bytes: 4 * 1024 * 1024,
            flush_interval_ms: 1000,
            max_queue_size: 2048,
            max_batch_size: 512,
            export_timeout_ms: 10_000,
        }
    }
}

/// Entry point: the synchronous, never-blocking side SDKs push into.
///
/// Fail-open by design: [`emit`](Self::emit) cannot fail or block — when the queue is full the
/// oldest event is dropped and counted in [`dropped_count`](Self::dropped_count). Cheap to
/// clone; every clone feeds the same pipeline.
///
/// ```
/// # use std::sync::Arc;
/// # use livekit_telemetry::*;
/// # struct Discard;
/// # #[async_trait::async_trait]
/// # impl TelemetryTransport for Discard {
/// #     async fn send(&self, _: ExportRequest) -> Result<(), ExportError> { Ok(()) }
/// # }
/// # #[tokio::main(flavor = "current_thread")] async fn main() {
/// let (telemetry, exporter) =
///     Telemetry::new(TelemetryConfig::new("http://localhost:4318/v1/logs"), Arc::new(Discard));
/// tokio::spawn(exporter.run());
///
/// telemetry.emit(TelemetryEvent::new("lk.ping"));
/// telemetry.shutdown().await;
/// # }
/// ```
#[derive(Clone)]
pub struct Telemetry {
    store: Arc<Store>,
    config: Arc<TelemetryConfig>,
    commands: mpsc::UnboundedSender<Command>,
}

impl Telemetry {
    /// Build the pipeline. Spawn the returned [`Exporter`] with `exporter.run()` on your runtime.
    ///
    /// An unusable `storage_dir` is logged and persistence is skipped — never an error.
    pub fn new(
        mut config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
    ) -> (Self, Exporter) {
        add_sdk_resource(&mut config.resource);
        let cache = config.storage_dir.as_deref().and_then(|dir| {
            FileCache::open(dir, config.max_storage_bytes)
                .map(Arc::new)
                .map_err(|err| {
                    log::warn!("telemetry: cannot use storage dir {dir}: {err}; not persisting")
                })
                .ok()
        });
        let config = Arc::new(config);
        let store = Arc::new(Store::new(config.max_queue_size.max(1) as usize));
        let (commands, receiver) = mpsc::unbounded_channel();
        let exporter = Exporter::new(store.clone(), transport, config.clone(), cache, receiver);
        (Self { store, config, commands }, exporter)
    }

    /// Queue an event for export. Stamps it with the current time unless it carries one.
    pub fn emit(&self, mut event: TelemetryEvent) {
        if event.timestamp_ns.is_none() {
            event.timestamp_ns = Some(now_unix_nanos());
        }
        self.store.push(event);
    }

    /// Export everything queued and wait until the transport accepted (or the exporter gave up on) it.
    pub async fn flush(&self) {
        self.command(Command::Flush).await;
    }

    /// Flush, then stop the exporter. Bounded by `export_timeout_ms`; events emitted afterwards
    /// are never exported. With `storage_dir` set, queued events are written to disk before the
    /// network is tried, so nothing is lost if the process dies mid-way.
    pub async fn shutdown(&self) {
        let bound = Duration::from_millis(self.config.export_timeout_ms.max(1));
        let _ = timeout(bound, self.command(Command::Shutdown)).await;
    }

    /// Events dropped so far (queue overflow, rejected/throttled/failed exports, disabled collector).
    pub fn dropped_count(&self) -> u64 {
        self.store.dropped()
    }

    async fn command(&self, make: impl FnOnce(oneshot::Sender<()>) -> Command) {
        let (done, wait) = oneshot::channel();
        if self.commands.send(make(done)).is_ok() {
            let _ = wait.await;
        }
    }
}

/// Fill in the `telemetry.sdk.*` resource attributes and a fallback `service.name`.
fn add_sdk_resource(resource: &mut Vec<Attribute>) {
    let defaults = [
        ("service.name", "livekit-client"),
        ("telemetry.sdk.name", env!("CARGO_PKG_NAME")),
        ("telemetry.sdk.language", "rust"),
        ("telemetry.sdk.version", env!("CARGO_PKG_VERSION")),
    ];
    for (key, value) in defaults {
        if !resource.iter().any(|a| a.key == key) {
            resource.push(Attribute::new(key, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, fs, path::Path, sync::Mutex};

    use prost::Message;

    use super::*;
    use crate::{
        persist::temp_dir,
        proto::opentelemetry::proto::collector::logs::v1::ExportLogsServiceRequest, ExportError,
        ExportRequest,
    };

    #[derive(Default)]
    struct FakeTransport {
        requests: Mutex<Vec<ExportRequest>>,
        script: Mutex<VecDeque<Result<(), ExportError>>>,
    }

    impl FakeTransport {
        fn scripted(results: impl IntoIterator<Item = Result<(), ExportError>>) -> Arc<Self> {
            Arc::new(Self {
                script: Mutex::new(results.into_iter().collect()),
                ..Default::default()
            })
        }
        fn sent(&self) -> Vec<ExportRequest> {
            self.requests.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl TelemetryTransport for FakeTransport {
        async fn send(&self, request: ExportRequest) -> Result<(), ExportError> {
            self.requests.lock().expect("lock").push(request);
            self.script.lock().expect("lock").pop_front().unwrap_or(Ok(()))
        }
    }

    fn offline() -> Result<(), ExportError> {
        Err(ExportError::Retryable { message: "offline".into(), retry_after_ms: None })
    }

    fn pipeline(transport: Arc<FakeTransport>) -> Telemetry {
        start(TelemetryConfig::new("http://collector/v1/logs"), transport)
    }

    fn persisted_pipeline(transport: Arc<FakeTransport>, dir: &Path) -> Telemetry {
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
        config.storage_dir = Some(dir.to_string_lossy().into_owned());
        start(config, transport)
    }

    fn start(config: TelemetryConfig, transport: Arc<FakeTransport>) -> Telemetry {
        let (telemetry, exporter) = Telemetry::new(config, transport);
        tokio::spawn(exporter.run());
        telemetry
    }

    fn files_in(dir: &Path) -> usize {
        fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
    }

    #[tokio::test(start_paused = true)]
    async fn batches_events_into_one_otlp_request() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
        }
        telemetry.flush().await;

        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, "http://collector/v1/logs");
        assert_eq!(sent[0].headers["Content-Type"], "application/x-protobuf");
        let decoded = ExportLogsServiceRequest::decode(&sent[0].body[..]).expect("valid OTLP");
        let records = &decoded.resource_logs[0].scope_logs[0].log_records;
        assert_eq!(records.len(), 3);
        assert!(records.iter().all(|r| r.event_name == "lk.ping" && r.time_unix_nano > 0));
        let resource = decoded.resource_logs[0].resource.as_ref().expect("resource");
        assert!(resource.attributes.iter().any(|kv| kv.key == "telemetry.sdk.name"));
        assert_eq!(telemetry.dropped_count(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_transient_failures_then_drops_without_storage() {
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(transport.sent().len(), 3);
        assert_eq!(telemetry.dropped_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_batch_is_dropped_without_retry() {
        let transport =
            FakeTransport::scripted([Err(ExportError::Rejected { message: "400".into() })]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.dropped_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn disabled_collector_silences_the_exporter() {
        let transport = FakeTransport::scripted([Err(ExportError::Disabled)]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.shutdown().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.dropped_count(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_batch_is_persisted_and_replayed_on_next_start() {
        let dir = temp_dir("replay");
        let first_transport = FakeTransport::scripted(std::iter::repeat_with(offline).take(12));
        let first = persisted_pipeline(first_transport.clone(), &dir);
        first.emit(TelemetryEvent::new("lk.ping"));
        first.flush().await;
        assert_eq!(first_transport.sent().len(), 3);
        assert_eq!(first.dropped_count(), 0, "persisted, not dropped");
        assert_eq!(files_in(&dir), 1);

        let second_transport = FakeTransport::scripted([]);
        let second = persisted_pipeline(second_transport.clone(), &dir);
        second.flush().await;
        let sent = second_transport.sent();
        assert_eq!(sent.len(), 1, "replayed on start");
        let decoded = ExportLogsServiceRequest::decode(&sent[0].body[..]).expect("valid OTLP");
        assert_eq!(decoded.resource_logs[0].scope_logs[0].log_records[0].event_name, "lk.ping");
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn throttled_batch_is_dropped_not_persisted() {
        let dir = temp_dir("throttle");
        let throttled =
            || Err(ExportError::Retryable { message: "429".into(), retry_after_ms: Some(1000) });
        let transport = FakeTransport::scripted([throttled(), throttled(), throttled()]);
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(telemetry.dropped_count(), 1);
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_offline_keeps_queue_on_disk() {
        let dir = temp_dir("spill");
        let transport = FakeTransport::scripted(std::iter::repeat_with(offline).take(12));
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.shutdown().await;

        assert_eq!(files_in(&dir), 1, "spilled before the network was tried, kept after it failed");
        assert_eq!(telemetry.dropped_count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
