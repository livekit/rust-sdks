use anyhow::Result;
use clap::{Parser, ValueEnum};
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit_api::access_token::{AccessToken, VideoGrants};
use serde::Serialize;
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

const ROOM_NAME: &str = "data-track-benchmark";
const DEFAULT_LOSSY_MIN_DRAIN_MS: u64 = 500;
const DEFAULT_RELIABLE_DRAIN_MS: u64 = 10_000;

struct TestHeader {
    seq: u32,
    timestamp_ms: u64,
}

impl TestHeader {
    const SIZE: usize = 12; // 4 bytes seq + 8 bytes timestamp

    fn encode(&self, buf: &mut [u8]) {
        buf[0..4].copy_from_slice(&self.seq.to_be_bytes());
        buf[4..12].copy_from_slice(&self.timestamp_ms.to_be_bytes());
    }

    fn decode(buf: &[u8]) -> Self {
        let seq = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        let timestamp_ms = u64::from_be_bytes(buf[4..12].try_into().unwrap());
        Self { seq, timestamp_ms }
    }
}

#[derive(Parser)]
#[command(
    about = "Data track benchmark — measures delivery ratio and latency across payload sizes and frequencies"
)]
struct Args {
    /// Comma-separated payload sizes in KiB (e.g. "1,4,16,64")
    #[arg(short, long)]
    sizes: String,

    /// Comma-separated send frequencies in Hz (e.g. "1,5,10,25,1000")
    #[arg(short, long)]
    frequencies: String,

    /// Seconds to send per (size, frequency) combination
    #[arg(short, long, default_value_t = 10)]
    duration: u64,

    /// LiveKit room name to use
    #[arg(short, long, default_value = ROOM_NAME)]
    room: String,

    /// LiveKit server URL (overrides LIVEKIT_URL env var)
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key (overrides LIVEKIT_API_KEY env var)
    #[arg(long, env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret (overrides LIVEKIT_API_SECRET env var)
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// Output file path for CSV results (stdout if omitted)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Data track reliability mode to benchmark
    #[arg(long, value_enum, default_value_t = ReliabilitySelection::Both)]
    reliability: ReliabilitySelection,

    /// Skip cells whose requested payload throughput exceeds this MiB/s cap
    #[arg(long)]
    max_expected_mibps: Option<f64>,

    /// Milliseconds to wait after each send run before collecting subscriber stats
    #[arg(long)]
    drain_ms: Option<u64>,

    /// Milliseconds to wait after reliable send runs before collecting subscriber stats
    #[arg(long)]
    reliable_drain_ms: Option<u64>,

    /// Output file path for per-frame latency samples
    #[arg(long)]
    latency_output: Option<PathBuf>,

    /// Output file path for per-send publisher samples
    #[arg(long)]
    publish_output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReliabilitySelection {
    Lossy,
    Reliable,
    Both,
}

impl ReliabilitySelection {
    fn modes(self) -> Vec<ReliabilityMode> {
        match self {
            Self::Lossy => vec![ReliabilityMode::Lossy],
            Self::Reliable => vec![ReliabilityMode::Reliable],
            Self::Both => vec![ReliabilityMode::Lossy, ReliabilityMode::Reliable],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReliabilityMode {
    Lossy,
    Reliable,
}

impl ReliabilityMode {
    fn as_data_track_reliability(self) -> DataTrackReliability {
        match self {
            Self::Lossy => DataTrackReliability::Lossy,
            Self::Reliable => DataTrackReliability::Reliable,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Lossy => "lossy",
            Self::Reliable => "reliable",
        }
    }
}

struct BenchResult {
    received: u64,
    unique_received: u64,
    duplicate: u64,
    out_of_order: u64,
    missing_sequence: u64,
    avg_latency_ms: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    min_latency_ms: f64,
    max_latency_ms: f64,
    latency_samples: Vec<LatencySampleRow>,
}

#[derive(Debug, Default)]
struct PublishStats {
    attempted: u64,
    sent: u64,
    failed: u64,
    elapsed_ms: f64,
    avg_send_wait_ms: f64,
    max_send_wait_ms: f64,
    publish_samples: Vec<PublishSampleRow>,
}

#[derive(Serialize)]
struct BenchRow {
    reliability: String,
    size_kb: u64,
    freq_hz: u64,
    duration_s: u64,
    attempted: u64,
    sent: u64,
    failed: u64,
    received: u64,
    unique_received: u64,
    duplicate: u64,
    out_of_order: u64,
    missing_sequence: u64,
    delivery_ratio: f64,
    unique_delivery_ratio: f64,
    avg_latency_ms: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    min_latency_ms: f64,
    max_latency_ms: f64,
    drain_ms: u64,
    elapsed_ms: f64,
    actual_send_hz: f64,
    avg_send_wait_ms: f64,
    max_send_wait_ms: f64,
    expected_mibps: f64,
    actual_mibps: f64,
}

#[derive(Serialize, Clone)]
struct LatencySampleRow {
    reliability: String,
    size_kb: u64,
    freq_hz: u64,
    duration_s: u64,
    run_id: u64,
    seq: u32,
    receive_index: u64,
    send_elapsed_ms: u64,
    receive_elapsed_ms: u64,
    latency_ms: f64,
    duplicate: bool,
    out_of_order: bool,
}

#[derive(Debug, Serialize, Clone)]
struct PublishSampleRow {
    reliability: String,
    size_kb: u64,
    freq_hz: u64,
    duration_s: u64,
    run_id: u64,
    seq: u32,
    frame_bytes: u64,
    send_elapsed_ms: u64,
    send_wait_ms: f64,
    sent: bool,
}

#[derive(Clone)]
struct RunInfo {
    reliability: String,
    size_kb: u64,
    freq_hz: u64,
    duration_s: u64,
    run_id: u64,
    run_start_ts: u64,
}

enum SubCommand {
    /// Begin a new run; only count frames with timestamp >= this value.
    StartRun {
        info: RunInfo,
    },
    Collect(oneshot::Sender<BenchResult>),
}

fn parse_nonzero_list(name: &str, s: &str) -> Result<Vec<u64>> {
    let mut values = Vec::new();
    for raw in s.split(',') {
        let raw = raw.trim();
        let value = raw
            .parse::<u64>()
            .map_err(|err| anyhow::anyhow!("invalid {name} value '{raw}': {err}"))?;
        if value == 0 {
            anyhow::bail!("{name} values must be greater than zero");
        }
        values.push(value);
    }
    if values.is_empty() {
        anyhow::bail!("{name} list must not be empty");
    }
    Ok(values)
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn expected_mibps(size_kb: u64, freq_hz: u64) -> f64 {
    (size_kb * freq_hz) as f64 / 1024.0
}

fn drain_for_run(
    reliability: ReliabilityMode,
    freq_hz: u64,
    drain_ms: Option<u64>,
    reliable_drain_ms: Option<u64>,
) -> Duration {
    let drain_ms = match drain_ms {
        Some(drain_ms) => drain_ms,
        None => match reliability {
            ReliabilityMode::Lossy => DEFAULT_LOSSY_MIN_DRAIN_MS.max(2000 / freq_hz),
            ReliabilityMode::Reliable => reliable_drain_ms.unwrap_or(DEFAULT_RELIABLE_DRAIN_MS),
        },
    };
    Duration::from_millis(drain_ms)
}

fn create_token(api_key: &str, api_secret: &str, room: &str, identity: &str) -> Result<String> {
    let token = AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_string(),
            can_publish: true,
            can_publish_data: true,
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()?;
    Ok(token)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let sizes = parse_nonzero_list("size", &args.sizes)?;
    let frequencies = parse_nonzero_list("frequency", &args.frequencies)?;

    let mut rows = Vec::new();
    let mut latency_samples = Vec::new();
    let mut publish_samples = Vec::new();
    for reliability in args.reliability.modes() {
        let result = run_reliability_matrix(&args, &sizes, &frequencies, reliability).await?;
        rows.extend(result.rows);
        latency_samples.extend(result.latency_samples);
        publish_samples.extend(result.publish_samples);
    }

    match &args.output {
        Some(path) => write_csv(&rows, std::fs::File::create(path)?)?,
        None => {
            let stdout = io::stdout();
            write_csv(&rows, stdout.lock())?;
        }
    }

    if let Some(path) = &args.latency_output {
        write_csv(&latency_samples, std::fs::File::create(path)?)?;
    }
    if let Some(path) = &args.publish_output {
        write_csv(&publish_samples, std::fs::File::create(path)?)?;
    }

    Ok(())
}

struct MatrixResult {
    rows: Vec<BenchRow>,
    latency_samples: Vec<LatencySampleRow>,
    publish_samples: Vec<PublishSampleRow>,
}

async fn run_reliability_matrix(
    args: &Args,
    sizes: &[u64],
    frequencies: &[u64],
    reliability: ReliabilityMode,
) -> Result<MatrixResult> {
    let room = format!("{}-{}", args.room, reliability.as_str());
    let publisher_identity = format!("bench-publisher-{}", reliability.as_str());
    let subscriber_identity = format!("bench-subscriber-{}", reliability.as_str());
    let track_name = format!("benchmark-{}", reliability.as_str());

    let pub_token = create_token(&args.api_key, &args.api_secret, &room, &publisher_identity)?;
    let sub_token = create_token(&args.api_key, &args.api_secret, &room, &subscriber_identity)?;

    let (sub_room, sub_events) =
        Room::connect(&args.url, &sub_token, RoomOptions::default()).await?;
    log::info!("Subscriber connected: reliability={}", reliability.as_str());

    let (pub_room, _) = Room::connect(&args.url, &pub_token, RoomOptions::default()).await?;
    log::info!("Publisher connected: reliability={}", reliability.as_str());

    let track_reliability = reliability.as_data_track_reliability();
    let track = pub_room
        .local_participant()
        .publish_data_track(DataTrackOptions::new(&track_name).reliability(track_reliability))
        .await?;
    log::info!("Data track published, waiting for subscriber to discover it...");

    let subscription = wait_for_subscription(sub_events, reliability, &track_name).await?;
    log::info!("Subscriber subscribed to data track: reliability={}", reliability.as_str());

    // Let the SFU fully establish the subscription pipeline.
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<SubCommand>();
    let sub_handle =
        tokio::spawn(subscriber_task(subscription, cmd_rx, args.latency_output.is_some()));

    let mut rows = Vec::new();
    let mut latency_samples = Vec::new();
    let mut publish_samples = Vec::new();
    let mut run_id = 0;
    for &size_kb in sizes {
        for &freq_hz in frequencies {
            let payload_size = (size_kb as usize) * 1024;
            if payload_size < TestHeader::SIZE {
                log::warn!("Skipping {size_kb} KiB @ {freq_hz} Hz: payload too small for header");
                continue;
            }
            let expected_throughput_mibps = expected_mibps(size_kb, freq_hz);
            if let Some(max_expected_mibps) = args.max_expected_mibps {
                if expected_throughput_mibps > max_expected_mibps {
                    log::info!(
                        "Skipping {size_kb} KiB @ {freq_hz} Hz: expected throughput {expected_throughput_mibps:.3} MiB/s exceeds cap {max_expected_mibps:.3} MiB/s"
                    );
                    continue;
                }
            }

            let run_start_ts = now_millis();
            run_id += 1;
            let run_info = RunInfo {
                reliability: reliability.as_str().to_string(),
                size_kb,
                freq_hz,
                duration_s: args.duration,
                run_id,
                run_start_ts,
            };
            cmd_tx.send(SubCommand::StartRun { info: run_info.clone() })?;

            let publish_stats = publish_loop(
                &track,
                track_reliability,
                payload_size,
                freq_hz,
                args.duration,
                &run_info,
                args.publish_output.is_some(),
            )
            .await;
            publish_samples.extend(publish_stats.publish_samples.iter().cloned());

            let drain = drain_for_run(reliability, freq_hz, args.drain_ms, args.reliable_drain_ms);
            tokio::time::sleep(drain).await;

            let (tx, rx) = oneshot::channel();
            cmd_tx.send(SubCommand::Collect(tx))?;
            let stats = rx.await?;
            latency_samples.extend(stats.latency_samples.iter().cloned());

            let ratio = if publish_stats.sent == 0 {
                0.0
            } else {
                stats.received as f64 / publish_stats.sent as f64
            };
            let unique_ratio = if publish_stats.sent == 0 {
                0.0
            } else {
                stats.unique_received as f64 / publish_stats.sent as f64
            };
            let actual_throughput_mibps = (payload_size as f64 * stats.unique_received as f64)
                / (1024.0 * 1024.0)
                / (publish_stats.elapsed_ms / 1000.0).max(f64::EPSILON);
            let actual_send_hz =
                publish_stats.attempted as f64 / (publish_stats.elapsed_ms / 1000.0).max(0.001);

            rows.push(BenchRow {
                reliability: reliability.as_str().to_string(),
                size_kb,
                freq_hz,
                duration_s: args.duration,
                attempted: publish_stats.attempted,
                sent: publish_stats.sent,
                failed: publish_stats.failed,
                received: stats.received,
                unique_received: stats.unique_received,
                duplicate: stats.duplicate,
                out_of_order: stats.out_of_order,
                missing_sequence: stats.missing_sequence,
                delivery_ratio: ratio,
                unique_delivery_ratio: unique_ratio,
                avg_latency_ms: stats.avg_latency_ms,
                p50_latency_ms: stats.p50_latency_ms,
                p95_latency_ms: stats.p95_latency_ms,
                p99_latency_ms: stats.p99_latency_ms,
                min_latency_ms: stats.min_latency_ms,
                max_latency_ms: stats.max_latency_ms,
                drain_ms: drain.as_millis() as u64,
                elapsed_ms: publish_stats.elapsed_ms,
                actual_send_hz,
                avg_send_wait_ms: publish_stats.avg_send_wait_ms,
                max_send_wait_ms: publish_stats.max_send_wait_ms,
                expected_mibps: expected_throughput_mibps,
                actual_mibps: actual_throughput_mibps,
            });
        }
    }

    drop(cmd_tx);
    let _ = sub_handle.await;
    close_room_with_timeout("publisher", &pub_room).await?;
    close_room_with_timeout("subscriber", &sub_room).await?;

    Ok(MatrixResult { rows, latency_samples, publish_samples })
}

async fn close_room_with_timeout(label: &str, room: &Room) -> Result<()> {
    match tokio::time::timeout(Duration::from_secs(5), room.close()).await {
        Ok(result) => result?,
        Err(_) => log::warn!("{label} room close timed out"),
    }
    Ok(())
}

fn write_csv<T: Serialize>(rows: &[T], writer: impl Write) -> csv::Result<()> {
    let mut writer = csv::Writer::from_writer(writer);
    for row in rows {
        writer.serialize(row)?;
    }
    writer.flush()?;
    Ok(())
}

async fn wait_for_subscription(
    mut events: mpsc::UnboundedReceiver<RoomEvent>,
    reliability: ReliabilityMode,
    track_name: &str,
) -> Result<DataTrackStream> {
    loop {
        let event = events.recv().await.ok_or(anyhow::anyhow!("event channel closed"))?;
        if let RoomEvent::DataTrackPublished(remote_track) = event {
            if remote_track.info().name() != track_name {
                continue;
            }
            let published_reliability = remote_track.info().reliability();
            if published_reliability != reliability.as_data_track_reliability() {
                anyhow::bail!(
                    "remote track reliability mismatch: expected {:?}, got {:?}",
                    reliability.as_data_track_reliability(),
                    published_reliability
                );
            }
            log::info!(
                "Remote track '{}' from '{}'",
                remote_track.info().name(),
                remote_track.publisher_identity()
            );
            return Ok(remote_track
                .subscribe_with_options(
                    DataTrackSubscribeOptions::default()
                        .with_reliability(reliability.as_data_track_reliability()),
                )
                .await?);
        }
    }
}

async fn publish_loop(
    track: &LocalDataTrack,
    reliability: DataTrackReliability,
    payload_size: usize,
    freq_hz: u64,
    duration_s: u64,
    run: &RunInfo,
    record_publish_samples: bool,
) -> PublishStats {
    let interval = Duration::from_secs_f64(1.0 / freq_hz as f64);
    let started_at = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(duration_s);

    let mut buf = vec![0u8; payload_size];
    rand::fill(&mut buf[TestHeader::SIZE..]);

    let mut seq: u32 = 0;
    let mut stats = PublishStats::default();
    let mut send_wait_sum_ms = 0.0;

    while Instant::now() < deadline {
        TestHeader { seq, timestamp_ms: now_millis() }.encode(&mut buf);
        let frame = DataTrackFrame::new(buf.clone());

        stats.attempted += 1;
        let send_started_at = Instant::now();
        let sent = match reliability {
            DataTrackReliability::Lossy => track.try_send(frame).is_ok(),
            DataTrackReliability::Reliable => {
                match tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    track.send_frame(frame),
                )
                .await
                {
                    Ok(result) => result.is_ok(),
                    Err(_) => false,
                }
            }
        };
        let send_wait_ms = send_started_at.elapsed().as_secs_f64() * 1000.0;
        send_wait_sum_ms += send_wait_ms;
        stats.max_send_wait_ms = stats.max_send_wait_ms.max(send_wait_ms);
        if sent {
            stats.sent += 1;
        } else {
            stats.failed += 1;
        }
        if record_publish_samples {
            stats.publish_samples.push(PublishSampleRow {
                reliability: run.reliability.clone(),
                size_kb: run.size_kb,
                freq_hz: run.freq_hz,
                duration_s: run.duration_s,
                run_id: run.run_id,
                seq,
                frame_bytes: payload_size as u64,
                send_elapsed_ms: now_millis().saturating_sub(run.run_start_ts),
                send_wait_ms,
                sent,
            });
        }

        seq = seq.wrapping_add(1);
        tokio::time::sleep(interval).await;
    }

    if stats.failed > 0 {
        log::warn!("push failures: {} (payload={payload_size}, freq={freq_hz}Hz)", stats.failed);
    }

    stats.elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
    stats.avg_send_wait_ms =
        if stats.attempted == 0 { 0.0 } else { send_wait_sum_ms / stats.attempted as f64 };
    stats
}

async fn subscriber_task(
    mut stream: DataTrackStream,
    mut cmd_rx: mpsc::UnboundedReceiver<SubCommand>,
    record_latency_samples: bool,
) {
    let mut received: u64 = 0;
    let mut latencies: Vec<f64> = Vec::new();
    let mut latency_samples: Vec<LatencySampleRow> = Vec::new();
    let mut seen_sequences = HashSet::new();
    let mut duplicate: u64 = 0;
    let mut out_of_order: u64 = 0;
    let mut last_sequence: Option<u32> = None;
    let mut min_sequence: Option<u32> = None;
    let mut max_sequence: Option<u32> = None;
    let mut current_run: Option<RunInfo> = None;

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(frame) = frame else { break };
                let payload = frame.payload();
                if payload.len() < TestHeader::SIZE {
                    continue;
                }

                let header = TestHeader::decode(&payload);
                let Some(run) = &current_run else {
                    continue;
                };

                // Discard frames from a previous run
                if header.timestamp_ms < run.run_start_ts {
                    continue;
                }

                let now = now_millis();
                let latency_ms = now.saturating_sub(header.timestamp_ms) as f64;

                received += 1;
                let is_duplicate = !seen_sequences.insert(header.seq);
                if is_duplicate {
                    duplicate += 1;
                }
                let mut is_out_of_order = false;
                if let Some(last) = last_sequence {
                    if header.seq <= last {
                        is_out_of_order = true;
                        out_of_order += 1;
                    }
                }
                last_sequence = Some(header.seq);
                min_sequence = Some(min_sequence.map_or(header.seq, |value| value.min(header.seq)));
                max_sequence = Some(max_sequence.map_or(header.seq, |value| value.max(header.seq)));
                latencies.push(latency_ms);
                if record_latency_samples {
                    latency_samples.push(LatencySampleRow {
                        reliability: run.reliability.clone(),
                        size_kb: run.size_kb,
                        freq_hz: run.freq_hz,
                        duration_s: run.duration_s,
                        run_id: run.run_id,
                        seq: header.seq,
                        receive_index: received,
                        send_elapsed_ms: header.timestamp_ms.saturating_sub(run.run_start_ts),
                        receive_elapsed_ms: now.saturating_sub(run.run_start_ts),
                        latency_ms,
                        duplicate: is_duplicate,
                        out_of_order: is_out_of_order,
                    });
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    SubCommand::StartRun { info } => {
                        current_run = Some(info);
                        received = 0;
                        latencies.clear();
                        latency_samples.clear();
                        seen_sequences.clear();
                        duplicate = 0;
                        out_of_order = 0;
                        last_sequence = None;
                        min_sequence = None;
                        max_sequence = None;
                    }
                    SubCommand::Collect(tx) => {
                        let unique_received = seen_sequences.len() as u64;
                        let missing_sequence = match (min_sequence, max_sequence) {
                            (Some(min), Some(max)) => {
                                u64::from(max.saturating_sub(min)) + 1 - unique_received
                            }
                            _ => 0,
                        };
                        let latency_stats = latency_summary(&latencies);
                        let result = BenchResult {
                            received,
                            unique_received,
                            duplicate,
                            out_of_order,
                            missing_sequence,
                            avg_latency_ms: latency_stats.avg,
                            p50_latency_ms: latency_stats.p50,
                            p95_latency_ms: latency_stats.p95,
                            p99_latency_ms: latency_stats.p99,
                            min_latency_ms: latency_stats.min,
                            max_latency_ms: latency_stats.max,
                            latency_samples: latency_samples.clone(),
                        };
                        let _ = tx.send(result);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Default, PartialEq)]
struct LatencySummary {
    avg: f64,
    p50: f64,
    p95: f64,
    p99: f64,
    min: f64,
    max: f64,
}

fn latency_summary(latencies: &[f64]) -> LatencySummary {
    if latencies.is_empty() {
        return LatencySummary::default();
    }

    let mut sorted = latencies.to_vec();
    sorted.sort_by(f64::total_cmp);
    let sum: f64 = sorted.iter().sum();
    LatencySummary {
        avg: sum / sorted.len() as f64,
        p50: percentile(&sorted, 0.50),
        p95: percentile(&sorted, 0.95),
        p99: percentile(&sorted, 0.99),
        min: *sorted.first().unwrap(),
        max: *sorted.last().unwrap(),
    }
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reliability_selection_expands_both_modes_in_order() {
        assert_eq!(
            ReliabilitySelection::Both.modes(),
            vec![ReliabilityMode::Lossy, ReliabilityMode::Reliable]
        );
        assert_eq!(ReliabilitySelection::Lossy.modes(), vec![ReliabilityMode::Lossy]);
        assert_eq!(ReliabilitySelection::Reliable.modes(), vec![ReliabilityMode::Reliable]);
    }

    #[test]
    fn header_round_trips() {
        let mut buf = [0u8; TestHeader::SIZE];
        TestHeader { seq: 42, timestamp_ms: 123456 }.encode(&mut buf);
        let decoded = TestHeader::decode(&buf);
        assert_eq!(decoded.seq, 42);
        assert_eq!(decoded.timestamp_ms, 123456);
    }

    #[test]
    fn latency_summary_reports_percentiles() {
        let summary = latency_summary(&[1.0, 5.0, 2.0, 3.0, 4.0]);
        assert_eq!(summary.avg, 3.0);
        assert_eq!(summary.p50, 3.0);
        assert_eq!(summary.p95, 5.0);
        assert_eq!(summary.p99, 5.0);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 5.0);
    }

    #[test]
    fn parse_nonzero_list_rejects_zero_values() {
        assert_eq!(parse_nonzero_list("frequency", "1, 5, 10").unwrap(), vec![1, 5, 10]);
        assert!(parse_nonzero_list("frequency", "1,0").is_err());
    }

    #[test]
    fn expected_mibps_uses_kib_payload_units() {
        assert_eq!(expected_mibps(1, 50_000), 48.828125);
        assert_eq!(expected_mibps(512, 100), 50.0);
        assert_eq!(expected_mibps(512, 1_000), 500.0);
    }

    #[test]
    fn drain_for_run_uses_longer_reliable_default() {
        assert_eq!(
            drain_for_run(ReliabilityMode::Lossy, 1, None, None),
            Duration::from_millis(2000)
        );
        assert_eq!(
            drain_for_run(ReliabilityMode::Lossy, 100, None, None),
            Duration::from_millis(DEFAULT_LOSSY_MIN_DRAIN_MS)
        );
        assert_eq!(
            drain_for_run(ReliabilityMode::Reliable, 100, None, None),
            Duration::from_millis(DEFAULT_RELIABLE_DRAIN_MS)
        );
        assert_eq!(
            drain_for_run(ReliabilityMode::Reliable, 100, None, Some(3000)),
            Duration::from_millis(3000)
        );
        assert_eq!(
            drain_for_run(ReliabilityMode::Reliable, 100, Some(250), Some(3000)),
            Duration::from_millis(250)
        );
    }
}
