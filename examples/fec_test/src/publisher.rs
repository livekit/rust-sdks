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

//! FlexFEC test publisher: publishes a synthetic animated video track,
//! optionally protected with FlexFEC, and writes 1 Hz send statistics to a
//! CSV file.

mod common;

use std::{fs::File, io::Write as _, time::Duration};

use anyhow::Result;
use clap::Parser;
use livekit::{
    options::{
        FlexFecOptions, FrameMetadataFeatures, TrackPublishOptions, VideoCodec, VideoEncoding,
    },
    track::{LocalTrack, LocalVideoTrack, TrackSource},
    webrtc::{
        stats::RtcStats,
        video_frame::{FrameMetadata, I420Buffer, VideoFrame, VideoRotation},
        video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    },
    Room, RoomOptions,
};

#[derive(Parser, Debug)]
#[command(about = "FlexFEC test publisher")]
struct Args {
    #[arg(long, default_value = "ws://localhost:7880")]
    url: String,
    #[arg(long, default_value = "devkey")]
    api_key: String,
    #[arg(long, default_value = "secret")]
    api_secret: String,
    #[arg(long, default_value = "fec-test")]
    room: String,
    #[arg(long, default_value = "publisher")]
    identity: String,
    /// enable FlexFEC protection for the published video
    #[arg(long)]
    fec: bool,
    /// percentage of the video bitrate spent on FEC
    #[arg(long, default_value_t = 20)]
    protection_percent: u8,
    /// frames per FEC protection block
    #[arg(long, default_value_t = 6)]
    max_fec_frames: u8,
    #[arg(long, default_value_t = 640)]
    width: u32,
    #[arg(long, default_value_t = 360)]
    height: u32,
    #[arg(long, default_value_t = 24)]
    fps: u32,
    /// target encoder bitrate in kbps
    #[arg(long, default_value_t = 1000)]
    bitrate: u64,
    /// run duration in seconds, 0 = forever
    #[arg(long, default_value_t = 0)]
    duration: u64,
    /// CSV output for 1 Hz send statistics
    #[arg(long, default_value = "publisher_stats.csv")]
    stats_out: String,
}

/// Animated I420 frame generator: a moving diagonal gradient plus a bouncing
/// high-contrast block, enough entropy for the encoder to hold its target
/// bitrate.
fn render_frame(buffer: &mut I420Buffer, width: usize, height: usize, frame_index: u32) {
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let chroma_width = buffer.chroma_width() as usize;
    let chroma_height = buffer.chroma_height() as usize;
    let (data_y, data_u, data_v) = buffer.data_mut();

    let t = frame_index as usize;
    for row in 0..height {
        let base = row * stride_y as usize;
        for col in 0..width {
            data_y[base + col] = (((col + row + t * 4) % 256) as u8).wrapping_add(16);
        }
    }
    for row in 0..chroma_height {
        let base_u = row * stride_u as usize;
        let base_v = row * stride_v as usize;
        for col in 0..chroma_width {
            data_u[base_u + col] = (128 + ((col + t * 2) % 64)) as u8;
            data_v[base_v + col] = (128 + ((row + t * 3) % 64)) as u8;
        }
    }

    // bouncing block
    let block = 64.min(width / 4);
    let bx = (t * 7) % (width - block);
    let by = (t * 5) % (height - block);
    for row in by..by + block {
        let base = row * stride_y as usize;
        for col in bx..bx + block {
            data_y[base + col] = if (row / 8 + col / 8) % 2 == 0 { 235 } else { 16 };
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let token = common::mint_token(&args.api_key, &args.api_secret, &args.room, &args.identity)?;

    let mut options = RoomOptions::default();
    if args.fec {
        options.flexfec = Some(FlexFecOptions {
            protection_percent: args.protection_percent,
            max_fec_frames: args.max_fec_frames,
            bursty_mask: false,
        });
    }

    let (room, mut events) = Room::connect(&args.url, &token, options).await?;
    log::info!("connected to room {} as {} (fec: {})", room.name(), args.identity, args.fec);

    let source =
        NativeVideoSource::new(VideoResolution { width: args.width, height: args.height }, false);
    let track =
        LocalVideoTrack::create_video_track(&args.identity, RtcVideoSource::Native(source.clone()));

    // embed a wall-clock capture timestamp + frame id in each frame so the
    // subscriber can measure capture-to-decode latency (FrameMetadataFeatures
    // is #[non_exhaustive], build it via Default)
    let mut frame_metadata_features = FrameMetadataFeatures::default();
    frame_metadata_features.user_timestamp = true;
    frame_metadata_features.frame_id = true;

    room.local_participant()
        .publish_track(
            LocalTrack::Video(track.clone()),
            TrackPublishOptions {
                source: TrackSource::Camera,
                video_codec: VideoCodec::VP8,
                simulcast: false,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: args.bitrate * 1000,
                    max_framerate: args.fps as f64,
                }),
                frame_metadata_features,
                ..Default::default()
            },
        )
        .await?;
    log::info!("published video track");
    // Readiness sentinel on stdout (always captured, independent of RUST_LOG):
    // the harness waits for this before starting subscribers, so a track is
    // available as soon as they join.
    println!("PUBLISHER_READY {}", args.identity);

    // frame pump
    let fps = args.fps.max(1);
    let (width, height) = (args.width, args.height);
    tokio::spawn(async move {
        let mut frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            frame_metadata: None,
            buffer: I420Buffer::new(width, height),
        };
        let started = std::time::Instant::now();
        let mut interval = tokio::time::interval(Duration::from_micros(1_000_000 / fps as u64));
        let mut frame_index = 0u32;
        loop {
            interval.tick().await;
            render_frame(&mut frame.buffer, width as usize, height as usize, frame_index);
            frame_index = frame_index.wrapping_add(1);
            frame.timestamp_us = started.elapsed().as_micros() as i64;
            // wall-clock capture time + frame id for end-to-end latency tracking
            frame.frame_metadata = Some(FrameMetadata {
                user_timestamp: Some(common::unix_time_micros()),
                frame_id: Some(frame_index),
                user_data: None,
            });
            source.capture_frame(&frame);
        }
    });

    // drain room events
    tokio::spawn(async move { while events.recv().await.is_some() {} });

    let mut csv = File::create(&args.stats_out)?;
    writeln!(
        csv,
        "time,packets_sent,bytes_sent,retransmitted_packets_sent,frames_encoded,\
         target_bitrate,sent_video_rate_bps,sent_fec_rate_bps,sent_nack_rate_bps,active_streams"
    )?;

    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let started = tokio::time::Instant::now();
    loop {
        ticker.tick().await;
        if args.duration > 0 && started.elapsed().as_secs() >= args.duration {
            break;
        }

        let mut packets_sent = 0u64;
        let mut bytes_sent = 0u64;
        let mut retransmitted = 0u64;
        let mut frames_encoded = 0u32;
        let mut target_bitrate = 0.0f64;
        if let Ok(stats) = track.get_stats().await {
            for stat in &stats {
                if let RtcStats::OutboundRtp(outbound) = stat {
                    if outbound.stream.kind == "video" {
                        packets_sent += outbound.sent.packets_sent;
                        bytes_sent += outbound.sent.bytes_sent;
                        retransmitted += outbound.outbound.retransmitted_packets_sent;
                        frames_encoded += outbound.outbound.frames_encoded;
                        target_bitrate = outbound.outbound.target_bitrate;
                    }
                }
            }
        }

        let fec = room.fec_sender_stats();
        writeln!(
            csv,
            "{:.3},{},{},{},{},{:.0},{},{},{},{}",
            common::unix_time_secs(),
            packets_sent,
            bytes_sent,
            retransmitted,
            frames_encoded,
            target_bitrate,
            fec.sent_video_rate_bps,
            fec.sent_fec_rate_bps,
            fec.sent_nack_rate_bps,
            fec.active_streams,
        )?;
        csv.flush()?;
    }

    room.close().await?;
    Ok(())
}
