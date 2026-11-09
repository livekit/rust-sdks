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

//! Real exporter handshakes, with cleanup completed before every outcome assertion.

use std::path::PathBuf;
use std::sync::Weak;

use super::*;
use crate::split::tests::{decoded, field, fixture, gzip, record};
use crate::{
    BatchCache, ExportResponse, FileCache, MemoryCache, Scope, Telemetry, TelemetryConfig,
};

struct Request {
    at: Instant,
    body: Vec<u8>,
    pending: Vec<String>,
    reply: oneshot::Sender<ExportResponse>,
}

struct Host {
    cache: Weak<dyn BatchCache>,
    sends: mpsc::Sender<Request>,
}

#[async_trait::async_trait]
impl TelemetryTransport for Host {
    async fn send(&self, request: ExportRequest) -> Result<ExportResponse, ExportError> {
        let (reply, receive) = oneshot::channel();
        let pending = self.cache.upgrade().map(|c| c.pending()).unwrap_or_default();
        self.sends
            .send(Request { at: Instant::now(), body: request.body, pending, reply })
            .await
            .map_err(|_| ExportError::Retryable {
            reason: "test host gone".into(),
            retry_after_ms: None,
        })?;
        receive.await.map_err(|_| ExportError::Retryable {
            reason: "test reply cancelled".into(),
            retry_after_ms: None,
        })
    }
}

struct Rig {
    telemetry: Telemetry,
    room: Scope,
    task: tokio::task::JoinHandle<()>,
    host: Arc<Host>,
    sends: mpsc::Receiver<Request>,
    directory: Option<PathBuf>,
    parent: String,
}

impl Rig {
    fn new(disk: bool, body: &[u8], traces: bool, count: u64) -> Self {
        Self::at_limit(disk, body, traces, count, 600)
    }

    fn at_limit(disk: bool, body: &[u8], traces: bool, count: u64, limit: u64) -> Self {
        let directory = disk.then(|| crate::cache::temp_dir("streaming-split"));
        let cache: Arc<dyn BatchCache> = match directory.as_ref() {
            Some(path) => Arc::new(FileCache::open(path, 64 * 1024 * 1024).expect("file cache")),
            None => Arc::new(MemoryCache::new(64 * 1024 * 1024)),
        };
        let (send, sends) = mpsc::channel(8);
        let host = Arc::new(Host { cache: Arc::downgrade(&cache), sends: send });
        let (telemetry, exporter) = Telemetry::with_cache(
            TelemetryConfig { max_batch_bytes: limit, ..Default::default() },
            host.clone(),
            cache,
        );
        *telemetry.shared.destinations.lock().unwrap() =
            crate::destination::Destinations::new(None);
        let room = telemetry.begin_scope();
        room.set_server("wss://a.livekit.cloud", &crate::destination::tests::granted(600));
        let signal = if traces { 't' } else { 'l' };
        let parent = format!(
            "{:020}-000001-{count}-{signal}-{}-a.livekit.cloud",
            now_unix_nanos(),
            room.state.hex()
        );
        telemetry.shared.cache.push(&parent, body).expect("original cached");
        let task = tokio::spawn(exporter.run());
        Self { telemetry, room, task, host, sends, directory, parent }
    }

    async fn next(&mut self) -> Result<Request, String> {
        timeout(Duration::from_secs(5), self.sends.recv())
            .await
            .ok()
            .flatten()
            .ok_or_else(|| "no host request before deadline".into())
    }

    async fn flush(&self) -> Result<(), String> {
        timeout(Duration::from_secs(5), self.telemetry.flush())
            .await
            .map_err(|_| "flush did not finish".into())
    }

    /// Revoke, purge, and join or abort+join before returning any verification outcome.
    async fn cleanup(self) -> bool {
        let Self { telemetry, room, mut task, host, sends, directory, .. } = self;
        let pipeline = Arc::downgrade(&telemetry.shared);
        let cache = host.cache.clone();
        let host_weak = Arc::downgrade(&host);
        drop(room);
        let purged = timeout(Duration::from_secs(2), telemetry.purge()).await == Ok(true);
        let joined = match timeout(Duration::from_secs(2), &mut task).await {
            Ok(result) => result.is_ok(),
            Err(_) => {
                task.abort();
                let _ = task.await;
                false
            }
        };
        drop((telemetry, host, sends));
        let destroyed = pipeline.upgrade().is_none()
            && cache.upgrade().is_none()
            && host_weak.upgrade().is_none();
        let removed = directory.is_none_or(|path| std::fs::remove_dir_all(path).is_ok());
        purged && joined && destroyed && removed
    }
}

fn answer(request: Request, status: u16) -> Result<(), String> {
    request
        .reply
        .send(ExportResponse { status, ..Default::default() })
        .map_err(|_| "exporter stopped before reply".into())
}

#[tokio::test(start_paused = true)]
async fn oversized_disabled_host_answer_retains_memory_and_file_batch_until_retry_ack() {
    for disk in [false, true] {
        let records = vec![record(400, false, 0, false), record(400, false, 1, false)];
        let body = gzip(&fixture(&records));
        let mut rig = Rig::new(disk, &body, false, 2);
        let facts = async {
            let first = rig.next().await?;
            let at = first.at;
            let initial = first.pending == [rig.parent.clone()];
            let mut oversized = vec![b'x'; 65_537];
            oversized.extend_from_slice(b"data recording is disabled by owner");
            first
                .reply
                .send(ExportResponse {
                    status: 403,
                    body: oversized,
                    headers: HashMap::from([("Retry-After".into(), "17".into())]),
                })
                .map_err(|_| "first response cancelled")?;
            rig.flush().await?;
            let retained = rig.telemetry.shared.cache.read(&rig.parent).ok() == Some(body.clone());
            tokio::time::advance(Duration::from_secs(16)).await;
            rig.flush().await?;
            let early = rig.sends.try_recv().is_ok();
            tokio::time::advance(Duration::from_secs(1)).await;
            let retry = rig.next().await?;
            let retry_ok = retry.at - at >= Duration::from_secs(17)
                && retry.body == body
                && retry.pending.contains(&rig.parent);
            answer(retry, 200)?;
            rig.flush().await?;
            Ok::<_, String>((
                initial,
                retained,
                early,
                retry_ok,
                rig.telemetry.shared.cache.read(&rig.parent).is_err(),
                rig.telemetry.shared.counters.snapshot().disabled,
            ))
        }
        .await;
        let clean = rig.cleanup().await;
        assert!(clean, "disk={disk}: task joined and all Weak owners destroyed before assertions");
        let (initial, retained, early, retry_ok, gone, disabled) = facts.expect("host handshake");
        assert!(initial && retained && !early && retry_ok && gone && disabled == 0, "disk={disk}");
    }
}

#[tokio::test(start_paused = true)]
async fn larger_than_budget_logs_and_spans_split_then_ack_in_memory_and_file() {
    for disk in [false, true] {
        for traces in [false, true] {
            for random in [false, true] {
                let records: Vec<_> = (0..4)
                    .map(|n| record(crate::split::DECODE_BUDGET / 3, traces, n, random))
                    .collect();
                let body = gzip(&fixture(&records));
                let mut rig = Rig::new(disk, &body, traces, 4);
                let facts = async {
                    let first = rig.next().await?;
                    let initial = first.pending == [rig.parent.clone()] && first.body == body;
                    answer(first, 413)?;
                    let first_half = rig.next().await?;
                    let ids =
                        [BatchId::half(&rig.parent, 'a', 2), BatchId::half(&rig.parent, 'b', 2)];
                    let first_ok = first_half.pending == ids
                        && decoded(&first_half.body) == fixture(&records[..2]);
                    answer(first_half, 200)?;
                    let second_half = rig.next().await?;
                    let second_ok = second_half.pending == [ids[1].clone()]
                        && decoded(&second_half.body) == fixture(&records[2..]);
                    answer(second_half, 200)?;
                    rig.flush().await?;
                    let stats = rig.telemetry.shared.counters.snapshot();
                    Ok::<_, String>((
                        initial,
                        first_ok,
                        second_ok,
                        rig.telemetry.shared.cache.pending().is_empty(),
                        stats.uploads_sent,
                        stats.oversized,
                    ))
                }
                .await;
                let clean = rig.cleanup().await;
                assert!(clean, "disk={disk} traces={traces}: cleanup");
                let (initial, first, second, empty, sent, oversized) =
                    facts.expect("413→200→200 handshake");
                assert!(initial && first && second && empty && sent == 2 && oversized == 0,
                "disk={disk} traces={traces} random={random}: {initial} {first} {second} {empty} {sent} {oversized}");
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn deferred_unsplittable_batch_reports_paused_while_other_uploads_succeed() {
    let original = gzip(&[0x0a, 0xff, 0xff]);
    let record = record(30, false, 0, false);
    let good = gzip(&fixture(&[record.clone(), record]));
    let mut rig = Rig::new(false, &original, false, 2);
    let good_id = rig.parent.replace("000001", "000002");
    rig.telemetry.shared.cache.push(&good_id, &good).expect("same-project second batch");
    let facts = async {
        answer(rig.next().await?, 413)?;
        let unrelated = rig.next().await?;
        answer(unrelated, 200)?;
        rig.flush().await?;
        let deferred = rig.telemetry.stats().status;
        tokio::time::advance(Duration::from_secs(60)).await;
        let again = rig.next().await?;
        let retried = again.body == original;
        answer(again, 200)?;
        rig.flush().await?;
        Ok::<_, String>((deferred, retried, rig.telemetry.stats().status))
    }
    .await;
    assert!(rig.cleanup().await, "cleanup");
    let (deferred, retried, delivered) = facts.expect("deferred status handshake");
    eprintln!("status while deferred={deferred} after retry ack={delivered}");
    assert_eq!(deferred, TelemetryStatus::Paused);
    assert!(retried);
    assert_eq!(delivered, TelemetryStatus::Ok);
}

#[tokio::test(start_paused = true)]
async fn unsupported_and_malformed_batches_wait_without_blocking_same_project_or_busy_loop() {
    for disk in [false, true] {
        for case in ["malformed", "resources", "scopes"] {
            let record = record(30, false, 0, false);
            let bytes = match case {
                "malformed" => vec![0x0a, 0xff, 0xff],
                "resources" => [
                    field(1, &field(2, &field(2, &record))),
                    field(1, &field(2, &field(2, &record))),
                ]
                .concat(),
                _ => {
                    field(1, &(0..8).flat_map(|_| field(2, &field(2, &record))).collect::<Vec<_>>())
                }
            };
            let original = gzip(&bytes);
            let good = gzip(&fixture(&[record.clone(), record]));
            let mut rig = Rig::new(disk, &original, false, if case == "scopes" { 8 } else { 2 });
            let good_id = rig.parent.replace("000001", "000002");
            rig.telemetry.shared.cache.push(&good_id, &good).expect("same-project second batch");
            let facts = async {
                let bad = rig.next().await?;
                let bad_first = bad.body == original;
                answer(bad, 413)?;
                let unrelated = rig.next().await?;
                let progressed = unrelated.body == good && unrelated.pending.contains(&rig.parent);
                answer(unrelated, 200)?;
                rig.flush().await?;
                tokio::time::advance(Duration::from_secs(59)).await;
                rig.flush().await?;
                let no_spin = rig.sends.try_recv().is_err();
                let retained =
                    rig.telemetry.shared.cache.read(&rig.parent).ok() == Some(original.clone());
                tokio::time::advance(Duration::from_secs(1)).await;
                let again = rig.next().await?;
                let retried = again.body == original;
                answer(again, 413)?;
                rig.flush().await?;
                let stats = rig.telemetry.shared.counters.snapshot();
                Ok::<_, String>((
                    bad_first,
                    progressed,
                    no_spin,
                    retained,
                    retried,
                    stats.oversized,
                ))
            }
            .await;
            let clean = rig.cleanup().await;
            assert!(clean, "disk={disk} case={case}: cleanup");
            let (first, progressed, no_spin, retained, retried, oversized) =
                facts.expect("deferred handshake");
            assert!(
                first && progressed && no_spin && retained && retried && oversized == 0,
                "disk={disk} case={case}: {first} {progressed} {no_spin} {retained} {oversized}"
            );
        }
    }
}

/// Simulates external eviction after the exporter read the parent, before replacement commits.
struct RemovedParent(Arc<dyn BatchCache>);
impl BatchCache for RemovedParent {
    fn push(&self, id: &str, body: &[u8]) -> std::io::Result<Vec<String>> {
        self.0.push(id, body)
    }
    fn replace(&self, id: &str, new: &[(String, Vec<u8>)]) -> std::io::Result<Vec<String>> {
        self.0.remove(id)?;
        self.0.replace(id, new)
    }
    fn pending(&self) -> Vec<String> {
        self.0.pending()
    }
    fn read(&self, id: &str) -> std::io::Result<Vec<u8>> {
        self.0.read(id)
    }
    fn remove(&self, id: &str) -> std::io::Result<()> {
        self.0.remove(id)
    }
    fn clear(&self) -> std::io::Result<()> {
        self.0.clear()
    }
}

#[tokio::test]
async fn evicted_parent_never_resurrects_children_or_retains_birth_metadata() {
    for disk in [false, true] {
        let directory = disk.then(|| crate::cache::temp_dir("split-eviction"));
        let cache: Arc<dyn BatchCache> = match &directory {
            Some(path) => Arc::new(FileCache::open(path, 64 * 1024 * 1024).expect("cache")),
            None => Arc::new(MemoryCache::new(64 * 1024 * 1024)),
        };
        let records = vec![record(400, false, 0, false), record(400, false, 1, false)];
        let body = gzip(&fixture(&records));
        let parent = format!("{:020}-000001-2-l-process-a.livekit.cloud", now_unix_nanos());
        cache.push(&parent, &body).expect("parent");
        let (send, _receives) = mpsc::channel(1);
        let host = Arc::new(Host { cache: Arc::downgrade(&cache), sends: send });
        let weak_cache = Arc::downgrade(&cache);
        let (telemetry, mut exporter) =
            Telemetry::with_cache(Default::default(), host.clone(), Arc::new(RemovedParent(cache)));
        let weak = Arc::downgrade(&telemetry.shared);
        exporter.born.insert(parent.clone(), Instant::now());
        let result = exporter.split(&parent, &body).map_err(|e| e.kind());
        let no_birth = !exporter.born.contains_key(&parent);
        let pending = telemetry.shared.cache.pending();
        let oversized = telemetry.shared.counters.snapshot().oversized;
        drop((exporter, telemetry, host));
        let destroyed = weak.upgrade().is_none() && weak_cache.upgrade().is_none();
        let removed = directory.is_none_or(|path| std::fs::remove_dir_all(path).is_ok());
        assert!(destroyed && removed);
        assert_eq!(result, Err(std::io::ErrorKind::NotFound));
        assert!(
            pending.is_empty() && no_birth && oversized == 0,
            "pending={pending:?} no_birth={no_birth} oversized={oversized}"
        );
    }
}

#[tokio::test]
async fn partial_file_replacement_failure_keeps_original_and_age_and_removes_temporary_outputs() {
    let directory = crate::cache::temp_dir("split-replacement-failure");
    let cache = Arc::new(FileCache::open(&directory, 64 * 1024 * 1024).expect("cache"));
    let records: Vec<_> =
        (0..4).map(|n| record(crate::split::DECODE_BUDGET / 3, false, n, false)).collect();
    let body = gzip(&fixture(&records));
    let parent = format!("{:020}-000001-4-l-process-a.livekit.cloud", now_unix_nanos());
    cache.push(&parent, &body).expect("parent");
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    cache.inject(move |step, _| {
        if step == crate::cache::Step::Rename
            && count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1
        {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    });
    let erased: Arc<dyn BatchCache> = cache.clone();
    let (send, _receives) = mpsc::channel(1);
    let host = Arc::new(Host { cache: Arc::downgrade(&erased), sends: send });
    let weak_cache = Arc::downgrade(&erased);
    let (telemetry, mut exporter) = Telemetry::with_cache(Default::default(), host.clone(), erased);
    let weak = Arc::downgrade(&telemetry.shared);
    let born = Instant::now();
    exporter.born.insert(parent.clone(), born);
    let result = exporter.split(&parent, &body).map_err(|e| e.kind());
    let original = cache.read(&parent).ok() == Some(body);
    let age = exporter.born.get(&parent).copied() == Some(born);
    let pending = cache.pending() == [parent];
    let files = std::fs::read_dir(&directory).expect("directory").filter_map(Result::ok).count();
    drop((exporter, telemetry, host, cache));
    let destroyed = weak.upgrade().is_none() && weak_cache.upgrade().is_none();
    let removed = std::fs::remove_dir_all(directory).is_ok();
    assert!(destroyed && removed);
    assert_eq!(result, Err(std::io::ErrorKind::PermissionDenied));
    assert!(
        original && age && pending && files == 1,
        "original={original} age={age} pending={pending} files={files}"
    );
}

#[tokio::test]
async fn opt_out_during_recovery_discards_children_and_public_purge_erases_original() {
    for disk in [false, true] {
        let directory = disk.then(|| crate::cache::temp_dir("split-consent"));
        let cache: Arc<dyn BatchCache> = match &directory {
            Some(path) => Arc::new(FileCache::open(path, 64 * 1024 * 1024).expect("cache")),
            None => Arc::new(MemoryCache::new(64 * 1024 * 1024)),
        };
        let records: Vec<_> =
            (0..4).map(|n| record(crate::split::DECODE_BUDGET / 3, false, n, false)).collect();
        let body = gzip(&fixture(&records));
        let parent = format!("{:020}-000001-4-l-process-a.livekit.cloud", now_unix_nanos());
        cache.push(&parent, &body).expect("parent");
        let calls = std::cell::Cell::new(0usize);
        let calibrated = crate::split::recover(&body, || {
            calls.set(calls.get() + 1);
            false
        });
        let threshold = calls.get().saturating_sub(10);
        drop(calibrated);
        calls.set(0);
        let (send, _receives) = mpsc::channel(1);
        let host = Arc::new(Host { cache: Arc::downgrade(&cache), sends: send });
        let weak_cache = Arc::downgrade(&cache);
        let (telemetry, exporter) = Telemetry::with_cache(Default::default(), host.clone(), cache);
        let weak = Arc::downgrade(&telemetry.shared);
        let result = crate::split::recover(&body, || {
            calls.set(calls.get() + 1);
            let Some(shared) = weak.upgrade() else {
                return true;
            };
            if calls.get() == threshold {
                shared.revoke();
            }
            shared.revoked()
        });
        let retained = telemetry.shared.cache.read(&parent).ok() == Some(body);
        let replacement =
            telemetry.shared.cache.replace(&parent, &[(parent.clone() + "a", vec![1])]);
        // This unit owns an unstarted exporter, so destruction (not an invented join) closes it.
        drop(exporter);
        let purged = telemetry.purge().await;
        let empty = telemetry.shared.cache.pending().is_empty();
        drop((telemetry, host));
        let destroyed = weak.upgrade().is_none() && weak_cache.upgrade().is_none();
        let removed = directory.is_none_or(|path| std::fs::remove_dir_all(path).is_ok());
        assert!(destroyed && removed && purged);
        assert_eq!(result, Err(crate::split::Failure::Cancelled));
        assert!(retained && replacement.is_err() && empty);
    }
}

/// Build the exact earlier 824-byte fixture without mutable production state.
fn legacy_824_body() -> Vec<u8> {
    use crate::proto::opentelemetry::proto::{
        collector::logs::v1::ExportLogsServiceRequest,
        common::v1::{any_value, AnyValue},
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
    };
    use prost::Message;
    ExportLogsServiceRequest {
        resource_logs: vec![ResourceLogs {
            scope_logs: vec![ScopeLogs {
                log_records: (0..2)
                    .map(|_| LogRecord {
                        body: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("x".repeat(400))),
                        }),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        }],
    }
    .encode_to_vec()
}

/// Replay the exact previously reviewed 824-byte FileCache fixture and its two controls.
#[tokio::test(start_paused = true)]
async fn persisted_824_bytes_replay_below_current_600_limit_and_controls() {
    let raw = legacy_824_body();
    assert_eq!(raw.len(), 824, "unchanged fixture before any tasks start");
    let body = gzip(&raw);
    for (limit, first_status) in [(600, 413), (2048, 413), (600, 200)] {
        let mut rig = Rig::at_limit(true, &body, false, 2, limit);
        let facts = async {
            let first = rig.next().await?;
            let mut sizes = vec![decoded(&first.body).len()];
            let mut committed = first
                .pending
                .iter()
                .any(|id| rig.telemetry.shared.cache.read(id).ok().as_ref() == Some(&first.body));
            answer(first, first_status)?;
            if first_status == 413 {
                for _ in 0..2 {
                    let half = rig.next().await?;
                    sizes.push(decoded(&half.body).len());
                    committed &= half.pending.iter().any(|id| {
                        rig.telemetry.shared.cache.read(id).ok().as_ref() == Some(&half.body)
                    });
                    answer(half, 200)?;
                }
            }
            rig.flush().await?;
            let stats = rig.telemetry.stats();
            Ok::<_, String>((
                sizes,
                committed,
                rig.telemetry.shared.cache.pending().is_empty(),
                stats.uploads_sent,
                stats.dropped,
                stats.dropped_corrupt,
            ))
        }
        .await;
        let clean = rig.cleanup().await;
        assert!(clean, "FileCache exporter joined and all Weak owners destroyed");
        let (sizes, committed, empty, sent, dropped, corrupt) = facts.expect("persisted replay");
        eprintln!("legacy 824 limit={limit} status={first_status} sizes={sizes:?} committed={committed} sent={sent}");
        assert_eq!(sizes, if first_status == 413 { vec![824, 415, 415] } else { vec![824] });
        assert!(committed && empty && dropped == 0 && corrupt == 0);
        assert_eq!(sent, if first_status == 413 { 2 } else { 1 });
    }
}

/// The same persisted fixture with a bad CRC follows the existing pre-send corruption policy.
#[tokio::test(start_paused = true)]
async fn persisted_bad_crc_is_not_sent_or_reclassified_as_a_split_failure() {
    let mut body = gzip(&legacy_824_body());
    let crc = body.len() - 8;
    body[crc] ^= 1;
    let mut rig = Rig::new(true, &body, false, 2);
    let facts = async {
        rig.flush().await?;
        let stats = rig.telemetry.stats();
        Ok::<_, String>((
            rig.sends.try_recv().is_err(),
            rig.telemetry.shared.cache.pending().is_empty(),
            stats.uploads_sent,
            stats.dropped_corrupt,
            stats.dropped_oversized,
        ))
    }
    .await;
    let clean = rig.cleanup().await;
    assert!(clean, "exporter joined and owners destroyed before outcome assertions");
    let (unsent, empty, sent, corrupt, oversized) = facts.expect("corruption control");
    assert!(unsent && empty && sent == 0 && corrupt == 2 && oversized == 0);
}
