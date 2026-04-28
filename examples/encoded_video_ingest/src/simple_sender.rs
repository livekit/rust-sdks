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

//! Minimal encoded (compressed) ingest driver using
//! [`livekit_encoded_video_ingest::EncodedTcpIngest`].
//!
//! Everything that was hand-rolled in `sender.rs` (demuxing, keyframe
//! detection, reconnect loop, observer plumbing) now lives inside the
//! SDK. This example is effectively: parse CLI args, connect to the
//! room, `EncodedTcpIngest::start`, log stats, wait for Ctrl-C.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::Result;
use clap::Parser;
use libwebrtc::video_source::VideoCodec;
use livekit::prelude::*;
use livekit_api::access_token;
use livekit_encoded_video_ingest::{
    EncodedIngestObserver, EncodedTcpIngest, EncodedTcpIngestOptions,
};
use log::{info, warn};
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit server URL (or set LIVEKIT_URL env var)
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key (or set LIVEKIT_API_KEY env var)
    #[arg(long, env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret (or set LIVEKIT_API_SECRET env var)
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// Room name to join
    #[arg(long, default_value = "encoded-video-demo")]
    room: String,

    /// Participant identity
    #[arg(long, default_value = "encoded-sender")]
    identity: String,

    /// Host of the gstreamer `tcpserversink`
    #[arg(long, default_value = "127.0.0.1")]
    tcp_host: String,

    /// Port of the gstreamer `tcpserversink`
    #[arg(long, default_value_t = 5000)]
    tcp_port: u16,

    /// Declared stream width (px)
    #[arg(long, default_value_t = 640)]
    width: u32,

    /// Declared stream height (px)
    #[arg(long, default_value_t = 480)]
    height: u32,

    /// Encoded (compressed) codec on the wire. Must match the gstreamer pipeline.
    #[arg(long, value_enum, default_value_t = CodecArg::H264)]
    codec: CodecArg,

    /// Optional max bitrate forwarded to TrackPublishOptions.video_encoding.
    #[arg(long)]
    max_bitrate_kbps: Option<u64>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
enum CodecArg {
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

impl CodecArg {
    fn webrtc_codec(self) -> VideoCodec {
        match self {
            CodecArg::H264 => VideoCodec::H264,
            CodecArg::H265 => VideoCodec::H265,
            CodecArg::Vp8 => VideoCodec::Vp8,
            CodecArg::Vp9 => VideoCodec::Vp9,
            CodecArg::Av1 => VideoCodec::Av1,
        }
    }
}

/// Logs the feedback events the SDK surfaces. Real producers should
/// react here — e.g. nudge their hardware encoder to emit an IDR on
/// `on_keyframe_requested`, or clamp their encoder to the advertised
/// `on_target_bitrate`.
struct LoggingObserver;

impl EncodedIngestObserver for LoggingObserver {
    fn on_connected(&self, peer: SocketAddr) {
        info!("ingest: connected to {peer}");
    }
    fn on_disconnected(&self, reason: &str) {
        warn!("ingest: disconnected: {reason}");
    }
    fn on_keyframe_requested(&self) {
        warn!(
            "ingest: keyframe requested by receiver — producer should emit a keyframe on the \
             next frame"
        );
    }
    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        info!(
            "ingest: target bitrate update: {} kbps @ {:.1} fps",
            bitrate_bps / 1000,
            framerate_fps
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let token = access_token::AccessToken::with_api_key(&args.api_key, &args.api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room.clone(),
            can_publish: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("connecting to LiveKit room '{}' as '{}'...", args.room, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = false;
    room_options.dynacast = false;
    let (room, _events) = Room::connect(&args.url, &token, room_options).await?;
    info!("connected: {} (sid {})", room.name(), room.sid().await);

    let mut opts = EncodedTcpIngestOptions::new(
        args.tcp_port,
        args.codec.webrtc_codec(),
        args.width,
        args.height,
    );
    opts.host = args.tcp_host.clone();
    opts.max_bitrate_bps = args.max_bitrate_kbps.map(|k| k * 1000);

    let ingest = EncodedTcpIngest::start(room.local_participant(), opts).await?;
    ingest.set_observer(Arc::new(LoggingObserver));
    info!("ingest: started track sid={}", ingest.track_sid());

    // Poll stats every 2s while the ingest runs.
    let ingest_for_stats = Arc::new(ingest);
    let stats_task = {
        let ingest = ingest_for_stats.clone();
        tokio::spawn(async move {
            let mut prev = ingest.stats();
            loop {
                sleep(Duration::from_secs(2)).await;
                let cur = ingest.stats();
                let ok = cur.frames_accepted.saturating_sub(prev.frames_accepted);
                let dropped = cur.frames_dropped.saturating_sub(prev.frames_dropped);
                let kf = cur.keyframes.saturating_sub(prev.keyframes);
                if ok + dropped > 0 {
                    info!(
                        "ingest: {:.1} fps accepted, {:.1} fps dropped, {kf} keyframes (total \
                         reconnects={})",
                        ok as f64 / 2.0,
                        dropped as f64 / 2.0,
                        cur.tcp_reconnects
                    );
                }
                prev = cur;
            }
        })
    };

    tokio::signal::ctrl_c().await.ok();
    info!("ctrl-c received, shutting down...");
    stats_task.abort();

    let ingest = Arc::try_unwrap(ingest_for_stats)
        .map_err(|_| anyhow::anyhow!("ingest still referenced"))?;
    ingest.stop().await;
    info!("done");
    Ok(())
}
