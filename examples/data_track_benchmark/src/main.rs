use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use livekit::prelude::*;
use livekit_api::access_token::{AccessToken, VideoGrants};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

const HEADER_SIZE: usize = 12; // 4 bytes seq + 8 bytes timestamp
const ROOM_NAME: &str = "data-track-benchmark";

#[derive(Parser)]
#[command(about = "Data track benchmark — measures delivery ratio and latency across payload sizes and frequencies")]
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
}

struct BenchResult {
    received: u64,
    avg_latency_ms: f64,
    min_latency_ms: f64,
    max_latency_ms: f64,
}

enum SubCommand {
    /// Begin a new run; only count frames with timestamp >= this value.
    StartRun { run_start_ts: u64 },
    Collect(oneshot::Sender<BenchResult>),
}

fn parse_list(s: &str) -> Vec<u64> {
    s.split(',')
        .map(|v| v.trim().parse::<u64>().expect("invalid number in list"))
        .collect()
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn encode_header(buf: &mut [u8], seq: u32, timestamp_ms: u64) {
    buf[0..4].copy_from_slice(&seq.to_be_bytes());
    buf[4..12].copy_from_slice(&timestamp_ms.to_be_bytes());
}

fn decode_header(buf: &[u8]) -> (u32, u64) {
    let seq = u32::from_be_bytes(buf[0..4].try_into().unwrap());
    let ts = u64::from_be_bytes(buf[4..12].try_into().unwrap());
    (seq, ts)
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

    let (_sub_room, sub_events) = Room::connect(&args.url, &sub_token, RoomOptions::default()).await?;
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
    tokio::spawn(subscriber_task(subscription, cmd_rx));

    println!("payload_size_kb,frequency_hz,duration_s,sent,received,delivery_ratio,avg_latency_ms,min_latency_ms,max_latency_ms");

    for &size_kb in &sizes {
        for &freq_hz in &frequencies {
            let payload_size = (size_kb as usize) * 1024;
            if payload_size < HEADER_SIZE {
                log::warn!(
                    "Skipping {size_kb} KiB @ {freq_hz} Hz: payload too small for header"
                );
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

            let ratio = if sent == 0 {
                0.0
            } else {
                stats.received as f64 / sent as f64
            };

            println!(
                "{size_kb},{freq_hz},{},{sent},{},{ratio:.2},{:.2},{:.2},{:.2}",
                args.duration,
                stats.received,
                stats.avg_latency_ms,
                stats.min_latency_ms,
                stats.max_latency_ms,
            );
        }
    }

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

async fn publish_loop(track: &LocalDataTrack, payload_size: usize, freq_hz: u64, duration_s: u64) -> u64 {
    let interval = Duration::from_secs_f64(1.0 / freq_hz as f64);
    let deadline = Instant::now() + Duration::from_secs(duration_s);

    let mut buf = vec![0u8; payload_size];
    rand::fill(&mut buf[HEADER_SIZE..]);

    let mut seq: u32 = 0;
    let mut sent: u64 = 0;
    let mut push_failed: u64 = 0;

    while Instant::now() < deadline {
        encode_header(&mut buf, seq, now_millis());
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
                if payload.len() < HEADER_SIZE {
                    continue;
                }

                let (_seq, ts) = decode_header(&payload);

                // Discard frames from a previous run
                if ts < run_start_ts {
                    continue;
                }

                let now = now_millis();
                let latency_ms = now.saturating_sub(ts) as f64;

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
