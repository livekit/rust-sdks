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
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use livekit_runtime::timeout;
use tokio::sync::{mpsc, oneshot};

use crate::{
    event::now_unix_nanos, exporter::Command, stats::Counters, store::Store, Attribute, BatchCache,
    DeviceState, Exporter, FileCache, MemoryCache, TelemetryEvent, TelemetryStats,
    TelemetryTransport,
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
    /// Directory for the on-disk batch cache (created if missing; its parent must exist).
    /// `None` keeps batches in memory only: they survive failed uploads, not the process.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub storage_dir: Option<String>,
    /// Cap on cached batches, in memory or on disk; the oldest are evicted first.
    #[cfg_attr(feature = "uniffi", uniffi(default = 4194304))]
    pub max_cache_bytes: u64,
    /// Base export cadence; stretched up to 4× by [`DeviceState::cadence_factor`].
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
    /// Defaults for the given endpoint; in-memory cache.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            headers: HashMap::new(),
            resource: Vec::new(),
            storage_dir: None,
            max_cache_bytes: 4 * 1024 * 1024,
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
/// oldest event is dropped and counted in [`stats`](Self::stats). Cheap to clone; every clone
/// feeds the same pipeline.
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
    cache: Arc<dyn BatchCache>,
    counters: Arc<Counters>,
    device: Arc<Mutex<Option<DeviceState>>>,
    commands: mpsc::UnboundedSender<Command>,
}

impl Telemetry {
    /// Build the pipeline with the cache the config asks for: a [`FileCache`] in `storage_dir`,
    /// or a [`MemoryCache`] when unset — or when the directory is unusable (logged, never an
    /// error). Spawn the returned [`Exporter`] with `exporter.run()` on your runtime.
    pub fn new(
        config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
    ) -> (Self, Exporter) {
        let cache: Arc<dyn BatchCache> = match config.storage_dir.as_deref() {
            Some(dir) => match FileCache::open(dir, config.max_cache_bytes) {
                Ok(cache) => Arc::new(cache),
                Err(err) => {
                    log::warn!("telemetry: cannot use storage dir {dir}: {err}; caching in memory");
                    Arc::new(MemoryCache::new(config.max_cache_bytes))
                }
            },
            None => Arc::new(MemoryCache::new(config.max_cache_bytes)),
        };
        Self::with_cache(config, transport, cache)
    }

    /// Build the pipeline around a caller-provided [`BatchCache`] (`storage_dir` is ignored).
    pub fn with_cache(
        mut config: TelemetryConfig,
        transport: Arc<dyn TelemetryTransport>,
        cache: Arc<dyn BatchCache>,
    ) -> (Self, Exporter) {
        add_sdk_resource(&mut config.resource);
        let config = Arc::new(config);
        let counters = Arc::new(Counters::default());
        let store = Arc::new(Store::new(config.max_queue_size.max(1) as usize, counters.clone()));
        let (commands, receiver) = mpsc::unbounded_channel();
        let exporter = Exporter::new(
            store.clone(),
            transport,
            config.clone(),
            cache.clone(),
            counters.clone(),
            receiver,
        );
        let telemetry = Self { store, config, cache, counters, device: Arc::default(), commands };
        (telemetry, exporter)
    }

    /// Queue an event for export. Stamps it with the current time unless it carries one.
    pub fn emit(&self, mut event: TelemetryEvent) {
        if event.timestamp_ns.is_none() {
            event.timestamp_ns = Some(now_unix_nanos());
        }
        self.store.push(event);
    }

    /// Tell the pipeline what the device looks like. Emits the `lk.device.*.changed` events for
    /// whatever differs from the last state (everything, the first time) and re-tunes the export
    /// cadence: thermal pressure, low-power mode and the background stretch it up to 4×; entering
    /// the background also flushes once right away.
    pub fn set_device_state(&self, state: DeviceState) {
        let mut previous = self.device.lock().unwrap_or_else(|e| e.into_inner());
        for event in state.change_events(previous.as_ref()) {
            self.emit(event);
        }
        *previous = Some(state);
        let _ = self.commands.send(Command::DeviceState(state));
    }

    /// Cache everything queued and upload what the transport accepts right now.
    pub async fn flush(&self) {
        self.command(Command::Flush).await;
    }

    /// Flush, then stop the exporter. Bounded by `export_timeout_ms`; events emitted afterwards
    /// are never exported. Queued events reach the cache before the network is tried, so with
    /// a [`FileCache`] nothing is lost if the process dies mid-way.
    pub async fn shutdown(&self) {
        let bound = Duration::from_millis(self.config.export_timeout_ms.max(1));
        let _ = timeout(bound, self.command(Command::Shutdown)).await;
    }

    /// Pipeline health: drops by reason, uploads, cached batches. The same numbers ride to the
    /// backend as `lk.telemetry.report` events whenever something went wrong.
    pub fn stats(&self) -> TelemetryStats {
        TelemetryStats::new(self.counters.snapshot(), self.cache.pending().len() as u64)
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
        cache::temp_dir,
        proto::opentelemetry::proto::{
            collector::logs::v1::ExportLogsServiceRequest, common::v1::any_value::Value,
            logs::v1::LogRecord,
        },
        AppState, ExportError, ExportRequest, ThermalState,
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

    fn offline_forever() -> impl Iterator<Item = Result<(), ExportError>> {
        std::iter::repeat_with(offline).take(64)
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

    fn records(request: &ExportRequest) -> Vec<LogRecord> {
        let decoded = ExportLogsServiceRequest::decode(&request.body[..]).expect("valid OTLP");
        decoded.resource_logs[0].scope_logs[0].log_records.clone()
    }

    fn event_names(request: &ExportRequest) -> Vec<String> {
        records(request).iter().map(|r| r.event_name.clone()).collect()
    }

    fn attribute(record: &LogRecord, key: &str) -> Option<Value> {
        record.attributes.iter().find(|kv| kv.key == key)?.value.as_ref()?.value.clone()
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
        assert_eq!(event_names(&sent[0]), ["lk.ping"; 3]);
        let decoded = ExportLogsServiceRequest::decode(&sent[0].body[..]).expect("valid OTLP");
        let resource = decoded.resource_logs[0].resource.as_ref().expect("resource");
        assert!(resource.attributes.iter().any(|kv| kv.key == "telemetry.sdk.name"));
        assert_eq!(telemetry.stats().dropped, 0);
        assert_eq!(telemetry.stats().uploads_sent, 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failed_upload_waits_in_memory_and_is_retried_after_backoff() {
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 3, "first attempt plus two retries");
        assert_eq!(telemetry.stats().dropped, 0, "kept in the memory cache, not dropped");
        assert_eq!(telemetry.stats().upload_failures, 3);
        assert_eq!(telemetry.stats().cached_batches, 1);

        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 3, "backoff: no upload right away");

        tokio::time::sleep(Duration::from_secs(61)).await;
        assert_eq!(transport.sent().len(), 4, "retried once the backoff elapsed");
        assert_eq!(event_names(&transport.sent()[3]), ["lk.ping"]);
        assert_eq!(telemetry.stats().cached_batches, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn self_telemetry_report_rides_along_after_problems() {
        let transport = FakeTransport::scripted([offline(), offline(), offline()]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        tokio::time::sleep(Duration::from_secs(61)).await; // backoff over, batch uploads

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let sent = transport.sent();
        let last = &sent[sent.len() - 1];
        assert_eq!(event_names(last), ["lk.ping", "lk.telemetry.report"]);
        let report = &records(last)[1];
        assert_eq!(attribute(report, "lk.telemetry.uploads.failed"), Some(Value::IntValue(3)));
        assert_eq!(attribute(report, "lk.telemetry.uploads.sent"), Some(Value::IntValue(1)));
        assert_eq!(attribute(report, "lk.telemetry.cache.batches"), Some(Value::IntValue(0)));
        assert_eq!(attribute(report, "lk.telemetry.dropped.queue_full"), None, "zeros omitted");

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(event_names(&sent[sent.len() - 1]), ["lk.ping"], "nothing new to report");
    }

    #[tokio::test(start_paused = true)]
    async fn queue_overflow_is_counted_by_reason() {
        let mut config = TelemetryConfig::new("http://collector/v1/logs");
        config.max_queue_size = 1;
        let telemetry = start(config, FakeTransport::scripted([]));
        for _ in 0..3 {
            telemetry.emit(TelemetryEvent::new("lk.ping"));
        }
        let stats = telemetry.stats();
        assert_eq!(stats.dropped_queue_full, 2);
        assert_eq!(stats.dropped, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn rejected_batch_is_dropped_without_retry() {
        let transport =
            FakeTransport::scripted([Err(ExportError::Rejected { message: "400".into() })]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;

        assert_eq!(transport.sent().len(), 1);
        assert_eq!(telemetry.stats().dropped_rejected, 1);
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
        assert_eq!(telemetry.stats().dropped_disabled, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn batch_is_written_before_upload_and_replayed_on_next_start() {
        let dir = temp_dir("replay");
        let first_transport = FakeTransport::scripted(offline_forever());
        let first = persisted_pipeline(first_transport.clone(), &dir);
        first.emit(TelemetryEvent::new("lk.ping"));
        first.flush().await;
        assert_eq!(first_transport.sent().len(), 3);
        assert_eq!(first.stats().dropped, 0);
        assert_eq!(files_in(&dir), 1, "written before the first attempt, kept after failure");

        let second_transport = FakeTransport::scripted([]);
        let second = persisted_pipeline(second_transport.clone(), &dir);
        second.flush().await;
        let sent = second_transport.sent();
        assert_eq!(sent.len(), 1, "replayed on start");
        assert_eq!(event_names(&sent[0]), ["lk.ping"]);
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn throttling_keeps_cached_batches_and_drops_new_ones() {
        let dir = temp_dir("throttle");
        let throttled =
            Err(ExportError::Retryable { message: "429".into(), retry_after_ms: Some(5_000) });
        let transport = FakeTransport::scripted([throttled]);
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(transport.sent().len(), 1, "no retries on Retry-After");
        assert_eq!(files_in(&dir), 1, "the throttled batch stays cached");
        assert_eq!(telemetry.stats().dropped, 0);

        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        assert_eq!(telemetry.stats().dropped_throttled, 1, "new batches dropped inside the window");
        assert_eq!(files_in(&dir), 1, "and never written");

        tokio::time::sleep(Duration::from_secs(6)).await;
        assert_eq!(transport.sent().len(), 2, "cached batch uploaded after Retry-After");
        assert_eq!(files_in(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_offline_keeps_queue_on_disk() {
        let dir = temp_dir("spill");
        let transport = FakeTransport::scripted(offline_forever());
        let telemetry = persisted_pipeline(transport.clone(), &dir);
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.shutdown().await;

        assert_eq!(files_in(&dir), 1, "cached before the network was tried, kept after it failed");
        assert_eq!(telemetry.stats().dropped, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test(start_paused = true)]
    async fn custom_cache_is_used_as_is() {
        let cache = Arc::new(MemoryCache::new(1 << 20));
        let transport = FakeTransport::scripted(offline_forever());
        let (telemetry, exporter) = Telemetry::with_cache(
            TelemetryConfig::new("http://collector/v1/logs"),
            transport,
            cache.clone(),
        );
        tokio::spawn(exporter.run());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.flush().await;
        let pending = cache.pending();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].ends_with("-1"), "id carries the event count: {}", pending[0]);
    }

    #[tokio::test(start_paused = true)]
    async fn device_state_emits_change_events_and_stretches_cadence() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.set_device_state(DeviceState {
            thermal: ThermalState::Critical,
            ..DeviceState::default()
        });
        telemetry.flush().await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1);
        let names = event_names(&sent[0]);
        assert!(names.contains(&"lk.device.thermal.changed".to_owned()), "{names:?}");
        assert_eq!(names.len(), 3, "initial value for every field");
        let thermal = records(&sent[0])
            .into_iter()
            .find(|r| r.event_name == "lk.device.thermal.changed")
            .expect("thermal event");
        assert_eq!(
            attribute(&thermal, "lk.device.thermal.state"),
            Some(Value::StringValue("critical".into()))
        );

        // 1 s base interval × 4 under critical thermal pressure.
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(transport.sent().len(), 1, "not yet: cadence stretched to 4 s");
        tokio::time::sleep(Duration::from_millis(2_500)).await;
        assert_eq!(transport.sent().len(), 2, "exported on the stretched tick");
    }

    #[tokio::test(start_paused = true)]
    async fn entering_background_flushes_immediately() {
        let transport = FakeTransport::scripted([]);
        let telemetry = pipeline(transport.clone());
        telemetry.emit(TelemetryEvent::new("lk.ping"));
        telemetry.set_device_state(DeviceState {
            app_state: AppState::Background,
            ..DeviceState::default()
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        let sent = transport.sent();
        assert_eq!(sent.len(), 1, "flushed on the state change, not on the tick");
        assert!(event_names(&sent[0]).contains(&"lk.ping".to_owned()));
    }
}
