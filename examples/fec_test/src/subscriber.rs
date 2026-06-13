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

//! FlexFEC test subscriber: subscribes to all video tracks in the room,
//! decodes them and writes per-track 1 Hz receive statistics (loss, FEC
//! usage, freezes) to a CSV file.

mod common;

use std::{fs::File, io::Write as _, sync::Arc, time::Duration};

use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use livekit::{
    options::FlexFecOptions,
    track::RemoteTrack,
    webrtc::{stats::RtcStats, video_stream::native::NativeVideoStream},
    Room, RoomEvent, RoomOptions,
};
use tokio::sync::Mutex;

#[derive(Parser, Debug)]
#[command(about = "FlexFEC test subscriber")]
struct Args {
    #[arg(long, default_value = "ws://localhost:7880")]
    url: String,
    #[arg(long, default_value = "devkey")]
    api_key: String,
    #[arg(long, default_value = "secret")]
    api_secret: String,
    #[arg(long, default_value = "fec-test")]
    room: String,
    #[arg(long, default_value = "subscriber")]
    identity: String,
    /// negotiate FlexFEC on the subscriber leg (required to receive the
    /// SFU's FEC repair stream)
    #[arg(long)]
    fec: bool,
    /// run duration in seconds, 0 = forever
    #[arg(long, default_value_t = 0)]
    duration: u64,
    /// CSV output for 1 Hz per-track receive statistics
    #[arg(long, default_value = "subscriber_stats.csv")]
    stats_out: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let token = common::mint_token(&args.api_key, &args.api_secret, &args.room, &args.identity)?;

    let mut options = RoomOptions::default();
    options.auto_subscribe = true;
    if args.fec {
        // enables the FlexFEC field trials so the subscriber accepts the
        // flexfec-03 repair stream offered by the SFU
        options.flexfec = Some(FlexFecOptions::default());
    }

    let (room, mut events) = Room::connect(&args.url, &token, options).await?;
    log::info!("connected to room {} as {} (fec: {})", room.name(), args.identity, args.fec);

    let mut csv = File::create(&args.stats_out)?;
    writeln!(
        csv,
        "time,publisher,ssrc,packets_received,packets_lost,jitter,bytes_received,\
         fec_packets_received,fec_bytes_received,fec_packets_discarded,\
         frames_decoded,frames_dropped,frames_per_second,frames_received_app,\
         freeze_count,total_freeze_duration,pause_count,nack_count,pli_count,fir_count,\
         jitter_buffer_delay,jitter_buffer_emitted_count,total_processing_delay,\
         e2e_latency_ms,e2e_samples"
    )?;
    let csv = Arc::new(Mutex::new(csv));

    let started = tokio::time::Instant::now();
    let deadline = async {
        if args.duration > 0 {
            tokio::time::sleep(Duration::from_secs(args.duration)).await;
        } else {
            futures::future::pending::<()>().await;
        }
    };
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => break,
            event = events.recv() => {
                let Some(event) = event else { break };
                if let RoomEvent::TrackSubscribed { track, participant, .. } = event {
                    if let RemoteTrack::Video(video_track) = track {
                        let publisher = participant.identity().to_string();
                        log::info!("subscribed to video track from {}", publisher);

                        // drain decoded frames, decoding is required for
                        // frame/freeze statistics and counts as the
                        // application level "frames received"
                        let frame_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
                        // capture-to-decode latency aggregator (reset each
                        // CSV row): sum of per-frame latencies + sample count
                        let lat_sum_us = Arc::new(std::sync::atomic::AtomicU64::new(0));
                        let lat_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

                        // wire the receiver-side packet trailer handler so
                        // decoded frames carry the publisher's capture
                        // timestamp; must happen before the stream is created
                        // (NativeVideoStream auto-picks-up the track handler)
                        let _timing = video_track.subscribe_timing_events();
                        let mut stream = NativeVideoStream::new(video_track.rtc_track());
                        let counter = frame_counter.clone();
                        let lsum = lat_sum_us.clone();
                        let lcount = lat_count.clone();
                        tokio::spawn(async move {
                            while let Some(frame) = stream.next().await {
                                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if let Some(ut) =
                                    frame.frame_metadata.as_ref().and_then(|m| m.user_timestamp)
                                {
                                    let lat = common::unix_time_micros().saturating_sub(ut);
                                    lsum.fetch_add(lat, std::sync::atomic::Ordering::Relaxed);
                                    lcount.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                            }
                        });

                        let csv = csv.clone();
                        tokio::spawn(async move {
                            let mut ticker = tokio::time::interval(Duration::from_secs(1));
                            loop {
                                ticker.tick().await;
                                let Ok(stats) = video_track.get_stats().await else { break };
                                for stat in &stats {
                                    let RtcStats::InboundRtp(inbound) = stat else { continue };
                                    if inbound.stream.kind != "video" {
                                        continue;
                                    }
                                    let frames_app =
                                        frame_counter.load(std::sync::atomic::Ordering::Relaxed);
                                    // windowed capture-to-decode latency
                                    let lat_sum =
                                        lat_sum_us.swap(0, std::sync::atomic::Ordering::Relaxed);
                                    let lat_n =
                                        lat_count.swap(0, std::sync::atomic::Ordering::Relaxed);
                                    let e2e_ms = if lat_n > 0 {
                                        (lat_sum as f64 / lat_n as f64) / 1000.0
                                    } else {
                                        0.0
                                    };
                                    let mut csv = csv.lock().await;
                                    let _ = writeln!(
                                        csv,
                                        "{:.3},{},{},{},{},{:.6},{},{},{},{},{},{},{:.2},{},{},{:.3},{},{},{},{},{:.3},{},{:.3},{:.2},{}",
                                        common::unix_time_secs(),
                                        publisher,
                                        inbound.stream.ssrc,
                                        inbound.received.packets_received,
                                        inbound.received.packets_lost,
                                        inbound.received.jitter,
                                        inbound.inbound.bytes_received,
                                        inbound.inbound.fec_packets_received,
                                        inbound.inbound.fec_bytes_received,
                                        inbound.inbound.fec_packets_discarded,
                                        inbound.inbound.frames_decoded,
                                        inbound.inbound.frames_dropped,
                                        inbound.inbound.frames_per_second,
                                        frames_app,
                                        inbound.inbound.freeze_count,
                                        inbound.inbound.total_freeze_duration,
                                        inbound.inbound.pause_count,
                                        inbound.inbound.nack_count,
                                        inbound.inbound.pli_count,
                                        inbound.inbound.fir_count,
                                        inbound.inbound.jitter_buffer_delay,
                                        inbound.inbound.jitter_buffer_emitted_count,
                                        inbound.inbound.total_processing_delay,
                                        e2e_ms,
                                        lat_n,
                                    );
                                    let _ = csv.flush();
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    log::info!("ran for {:?}, closing", started.elapsed());
    room.close().await?;
    Ok(())
}
