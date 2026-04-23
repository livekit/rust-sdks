use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit_api::access_token::{AccessToken, VideoGrants};
use polars::prelude::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

const ROOM_NAME: &str = "data-track-benchmark";

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

    /// Output raw CSV instead of a human-readable table
    #[arg(long, default_value_t = false)]
    csv: bool,
}

struct BenchResult {
    received: u64,
    avg_latency_ms: f64,
    min_latency_ms: f64,
    max_latency_ms: f64,
}

enum SubCommand {
    /// Begin a new run; only count frames with timestamp >= this value.
    StartRun {
        run_start_ts: u64,
    },
    Collect(oneshot::Sender<BenchResult>),
}

fn parse_list(s: &str) -> Vec<u64> {
    s.split(',').map(|v| v.trim().parse::<u64>().expect("invalid number in list")).collect()
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
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
    let sizes = parse_list(&args.sizes);
    let frequencies = parse_list(&args.frequencies);

    let pub_token = create_token(&args.api_key, &args.api_secret, &args.room, "bench-publisher")?;
    let sub_token = create_token(&args.api_key, &args.api_secret, &args.room, "bench-subscriber")?;

    let (sub_room, sub_events) =
        Room::connect(&args.url, &sub_token, RoomOptions::default()).await?;
    log::info!("Subscriber connected");

    let (pub_room, _) = Room::connect(&args.url, &pub_token, RoomOptions::default()).await?;
    log::info!("Publisher connected");

    let track = pub_room.local_participant().publish_data_track("benchmark").await?;
    log::info!("Data track published, waiting for subscriber to discover it...");

    let subscription = wait_for_subscription(sub_events).await?;
    log::info!("Subscriber subscribed to data track");

    // Let the SFU fully establish the subscription pipeline
    tokio::time::sleep(Duration::from_secs(1)).await;

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<SubCommand>();
    let sub_handle = tokio::spawn(subscriber_task(subscription, cmd_rx));

    let mut col_size_kb: Vec<u64> = Vec::new();
    let mut col_freq_hz: Vec<u64> = Vec::new();
    let mut col_duration_s: Vec<u64> = Vec::new();
    let mut col_sent: Vec<u64> = Vec::new();
    let mut col_received: Vec<u64> = Vec::new();
    let mut col_ratio: Vec<f64> = Vec::new();
    let mut col_avg_ms: Vec<f64> = Vec::new();
    let mut col_min_ms: Vec<f64> = Vec::new();
    let mut col_max_ms: Vec<f64> = Vec::new();
    let mut col_expected_mibps: Vec<f64> = Vec::new();
    let mut col_actual_mibps: Vec<f64> = Vec::new();

    for &size_kb in &sizes {
        for &freq_hz in &frequencies {
            let payload_size = (size_kb as usize) * 1024;
            if payload_size < TestHeader::SIZE {
                log::warn!("Skipping {size_kb} KiB @ {freq_hz} Hz: payload too small for header");
                continue;
            }

            let run_start_ts = now_millis();
            cmd_tx.send(SubCommand::StartRun { run_start_ts })?;

            let sent = publish_loop(&track, payload_size, freq_hz, args.duration).await;

            let drain = Duration::from_millis(500.max(2000 / freq_hz));
            tokio::time::sleep(drain).await;

            let (tx, rx) = oneshot::channel();
            cmd_tx.send(SubCommand::Collect(tx))?;
            let stats = rx.await?;

            let ratio = if sent == 0 { 0.0 } else { stats.received as f64 / sent as f64 };
            let expected_throughput_mibps = (size_kb * freq_hz) as f64 / 1024.0;
            let actual_throughput_mibps = expected_throughput_mibps * ratio;

            col_size_kb.push(size_kb);
            col_freq_hz.push(freq_hz);
            col_duration_s.push(args.duration);
            col_sent.push(sent);
            col_received.push(stats.received);
            col_ratio.push(ratio);
            col_avg_ms.push(stats.avg_latency_ms);
            col_min_ms.push(stats.min_latency_ms);
            col_max_ms.push(stats.max_latency_ms);
            col_expected_mibps.push(expected_throughput_mibps);
            col_actual_mibps.push(actual_throughput_mibps);
        }
    }

    let mut df = df! [
        "size_kb"              => col_size_kb,
        "freq_hz"              => col_freq_hz,
        "duration_s"           => col_duration_s,
        "sent"                 => col_sent,
        "received"             => col_received,
        "delivery_ratio"       => col_ratio,
        "avg_latency_ms"       => col_avg_ms,
        "min_latency_ms"       => col_min_ms,
        "max_latency_ms"       => col_max_ms,
        "expected_mibps"       => col_expected_mibps,
        "actual_mibps"         => col_actual_mibps,
    ]?;

    if args.csv {
        CsvWriter::new(std::io::stdout()).include_header(true).finish(&mut df)?;
    } else {
        let n = df.height();
        let mut size_str = Vec::with_capacity(n);
        let mut freq_str = Vec::with_capacity(n);
        let mut dur_str = Vec::with_capacity(n);
        let mut sent_str = Vec::with_capacity(n);
        let mut received_str = Vec::with_capacity(n);
        let mut delivery_str = Vec::with_capacity(n);
        let mut avg_str = Vec::with_capacity(n);
        let mut min_str = Vec::with_capacity(n);
        let mut max_str = Vec::with_capacity(n);
        let mut exp_str = Vec::with_capacity(n);
        let mut act_str = Vec::with_capacity(n);

        for i in 0..n {
            let size_kb = df.column("size_kb")?.u64()?.get(i).unwrap();
            let freq_hz = df.column("freq_hz")?.u64()?.get(i).unwrap();
            let dur = df.column("duration_s")?.u64()?.get(i).unwrap();
            let sent = df.column("sent")?.u64()?.get(i).unwrap();
            let received = df.column("received")?.u64()?.get(i).unwrap();
            let ratio = df.column("delivery_ratio")?.f64()?.get(i).unwrap();
            let avg = df.column("avg_latency_ms")?.f64()?.get(i).unwrap();
            let min = df.column("min_latency_ms")?.f64()?.get(i).unwrap();
            let max = df.column("max_latency_ms")?.f64()?.get(i).unwrap();
            let exp_mibps = df.column("expected_mibps")?.f64()?.get(i).unwrap();
            let act_mibps = df.column("actual_mibps")?.f64()?.get(i).unwrap();

            size_str.push(size_kb.to_string());
            freq_str.push(freq_hz.to_string());
            dur_str.push(dur.to_string());
            sent_str.push(sent.to_string());
            received_str.push(received.to_string());
            delivery_str.push(format!("{:.2}%", ratio * 100.0));
            avg_str.push(format!("{avg:.2}"));
            min_str.push(format!("{min:.2}"));
            max_str.push(format!("{max:.2}"));
            exp_str.push(format!("{exp_mibps:.2} MiB/s"));
            act_str.push(format!("{act_mibps:.2} MiB/s"));
        }

        let display_df = df! [
            "size (KiB)"        => size_str,
            "freq (Hz)"         => freq_str,
            "dur (s)"           => dur_str,
            "sent"              => sent_str,
            "received"          => received_str,
            "delivery"          => delivery_str,
            "avg (ms)"          => avg_str,
            "min (ms)"          => min_str,
            "max (ms)"          => max_str,
            "exp throughput"    => exp_str,
            "actual throughput" => act_str,
        ]?;

        std::env::set_var("POLARS_FMT_MAX_ROWS", "-1");
        std::env::set_var("POLARS_FMT_MAX_COLS", "-1");
        std::env::set_var("POLARS_FMT_STR_LEN", "50");
        std::env::set_var("POLARS_TABLE_WIDTH", "500");
        std::env::set_var("POLARS_FMT_TABLE_FORMATTING", "ASCII_FULL_CONDENSED");
        std::env::set_var("POLARS_FMT_TABLE_CELL_ALIGNMENT", "RIGHT");
        std::env::set_var("POLARS_FMT_TABLE_HIDE_COLUMN_DATA_TYPES", "1");
        std::env::set_var("POLARS_FMT_TABLE_HIDE_DATAFRAME_SHAPE_INFORMATION", "1");

        println!("{display_df}");
    }

    drop(cmd_tx);
    let _ = sub_handle.await;
    pub_room.close().await?;
    sub_room.close().await?;

    Ok(())
}

async fn wait_for_subscription(
    mut events: mpsc::UnboundedReceiver<RoomEvent>,
) -> Result<DataTrackStream> {
    loop {
        let event = events.recv().await.ok_or(anyhow::anyhow!("event channel closed"))?;
        if let RoomEvent::DataTrackPublished(remote_track) = event {
            log::info!(
                "Remote track '{}' from '{}'",
                remote_track.info().name(),
                remote_track.publisher_identity()
            );
            return Ok(remote_track.subscribe().await?);
        }
    }
}

async fn publish_loop(
    track: &LocalDataTrack,
    payload_size: usize,
    freq_hz: u64,
    duration_s: u64,
) -> u64 {
    let interval = Duration::from_secs_f64(1.0 / freq_hz as f64);
    let deadline = Instant::now() + Duration::from_secs(duration_s);

    let mut buf = vec![0u8; payload_size];
    rand::fill(&mut buf[TestHeader::SIZE..]);

    let mut seq: u32 = 0;
    let mut sent: u64 = 0;
    let mut push_failed: u64 = 0;

    while Instant::now() < deadline {
        TestHeader { seq, timestamp_ms: now_millis() }.encode(&mut buf);
        let frame = DataTrackFrame::new(buf.clone());

        match track.try_push(frame) {
            Ok(()) => sent += 1,
            Err(_) => push_failed += 1,
        }

        seq = seq.wrapping_add(1);
        tokio::time::sleep(interval).await;
    }

    if push_failed > 0 {
        log::warn!("push failures: {push_failed} (payload={payload_size}, freq={freq_hz}Hz)");
    }

    sent
}

async fn subscriber_task(
    mut stream: DataTrackStream,
    mut cmd_rx: mpsc::UnboundedReceiver<SubCommand>,
) {
    let mut received: u64 = 0;
    let mut latencies: Vec<f64> = Vec::new();
    let mut run_start_ts: u64 = 0;

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(frame) = frame else { break };
                let payload = frame.payload();
                if payload.len() < TestHeader::SIZE {
                    continue;
                }

                let header = TestHeader::decode(&payload);

                // Discard frames from a previous run
                if header.timestamp_ms < run_start_ts {
                    continue;
                }

                let now = now_millis();
                let latency_ms = now.saturating_sub(header.timestamp_ms) as f64;

                received += 1;
                latencies.push(latency_ms);
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    SubCommand::StartRun { run_start_ts: ts } => {
                        run_start_ts = ts;
                        received = 0;
                        latencies.clear();
                    }
                    SubCommand::Collect(tx) => {
                        let result = if latencies.is_empty() {
                            BenchResult {
                                received,
                                avg_latency_ms: 0.0,
                                min_latency_ms: 0.0,
                                max_latency_ms: 0.0,
                            }
                        } else {
                            let sum: f64 = latencies.iter().sum();
                            let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
                            let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                            BenchResult {
                                received,
                                avg_latency_ms: sum / latencies.len() as f64,
                                min_latency_ms: min,
                                max_latency_ms: max,
                            }
                        };
                        let _ = tx.send(result);
                    }
                }
            }
        }
    }
}
