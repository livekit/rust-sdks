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
//
//! Pre-encoded H.264 ingest example.
//!
//! Connects to a TCP server emitting an Annex-B H.264 elementary stream
//! (e.g. gstreamer's `tcpserversink`), parses NALUs into access units, and
//! pushes each frame onto a LiveKit video track using the new pre-encoded
//! source API -- no re-encoding happens on the publisher side.
//!
//! # Example gstreamer pipeline (server side)
//!
//! ```text
//! gst-launch-1.0 -v videotestsrc is-live=true \
//!   ! video/x-raw,width=1280,height=720,framerate=30/1 \
//!   ! x264enc tune=zerolatency key-int-max=30 bitrate=2000 \
//!   ! video/x-h264,stream-format=byte-stream,alignment=au \
//!   ! tcpserversink host=0.0.0.0 port=5000
//! ```
//!
//! Then run this example to publish into a LiveKit room:
//!
//! ```text
//! cargo run --bin preencoded_ingest -- \
//!   --url wss://your.livekit.host \
//!   --api-key <KEY> --api-secret <SECRET> \
//!   --room demo \
//!   --connect 127.0.0.1:5000
//! ```

mod h264;
mod source;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use clap::Parser;
use libwebrtc::prelude::{EncodedFrameInfo, VideoCodecType};
use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    track::LocalVideoTrack,
    webrtc::{encoded_video_source::native::NativeEncodedVideoSource, video_source::RtcVideoSource},
};
use livekit_api::access_token;

use crate::source::{EncodedFrameSource, TcpH264Source};

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest pre-encoded H.264 video into LiveKit")]
struct Args {
    /// LiveKit server URL (e.g. wss://your.livekit.host)
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key
    #[arg(long, env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// Room name to join
    #[arg(long, default_value = "preencoded-demo")]
    room: String,

    /// `host:port` of the TCP server to read the H.264 stream from.
    #[arg(long, default_value = "127.0.0.1:5000")]
    connect: String,

    /// Declared video width (must match the encoded stream's resolution).
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Declared video height (must match the encoded stream's resolution).
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Track name surfaced to subscribers.
    #[arg(long, default_value = "preencoded")]
    track_name: String,

    /// Identity to publish under.
    #[arg(long, default_value = "preencoded-publisher")]
    identity: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let source = NativeEncodedVideoSource::new(args.width, args.height, VideoCodecType::H264);

    let video_track = LocalVideoTrack::create_video_track(
        &args.track_name,
        RtcVideoSource::Encoded(source.clone()),
    );

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

    log::info!("Connecting to room '{}' as '{}'", args.room, args.identity);
    let (room, mut events) = Room::connect(&args.url, &token, RoomOptions::default()).await?;
    log::info!("Connected: {} ({})", room.name(), room.sid().await);

    let publication = room
        .local_participant()
        .publish_track(
            LocalTrack::Video(video_track),
            TrackPublishOptions {
                source: TrackSource::Camera,
                ..Default::default()
            },
        )
        .await?;
    log::info!("Published track {} ({}x{} H.264)", publication.sid(), args.width, args.height);

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            log::info!("Ctrl-C received, shutting down...");
            running.store(false, Ordering::SeqCst);
        });
    }

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            log::debug!("Room event: {:?}", event);
        }
    });

    // The TCP+H.264 ingest is hidden behind the `EncodedFrameSource` trait
    // -- swap this constructor for a different implementation (file, named
    // pipe, gRPC, ...) without touching the rest of the loop.
    let mut frames: Box<dyn EncodedFrameSource> =
        Box::new(TcpH264Source::connect(&args.connect).await?);

    let mut total_frames: u64 = 0;
    let mut keyframes: u64 = 0;
    log::info!("Ingest loop started (waiting for first keyframe)...");
    while running.load(Ordering::SeqCst) {
        match frames.next_frame().await? {
            None => {
                log::info!("Source returned EOF");
                break;
            }
            Some(frame) => {
                if frame.is_keyframe {
                    keyframes += 1;
                }
                total_frames += 1;

                // Defaults: SDK fills FrameMetadata with the current system
                // timestamp + an auto-incrementing frame_id.  Callers that
                // need explicit packet-trailer values can use
                // `source.capture_frame_with_metadata(...)` instead.
                let is_keyframe = frame.is_keyframe;
                let has_parameter_sets = frame.has_parameter_sets;
                let info = EncodedFrameInfo {
                    data: frame.data,
                    is_keyframe,
                    has_sps_pps: has_parameter_sets,
                };
                let bytes = info.data.len();
                let ok = source.capture_frame(&info);

                if total_frames <= 5 || is_keyframe || total_frames.is_multiple_of(100) {
                    log::info!(
                        "Frame #{} accepted={} keyframe={} sps_pps={} bytes={}",
                        total_frames,
                        ok,
                        is_keyframe,
                        has_parameter_sets,
                        bytes,
                    );
                }
            }
        }
    }

    log::info!("Total frames: {} (keyframes: {})", total_frames, keyframes);
    room.close().await?;
    log::info!("Room closed");
    Ok(())
}
