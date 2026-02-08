// Copyright 2025 LiveKit, Inc.
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

//! Example: Ingest pre-encoded H264 video from a TCP server into a LiveKit room.
//!
//! # Usage
//!
//! First start a TCP server that streams Annex-B H264 data (e.g. with ffmpeg).
//!
//! **Important**: use `-g 30` (or similar) so that keyframes are emitted
//! frequently enough for subscribers to start receiving video quickly.
//! The `-x264opts keyint=30:min-keyint=30` flags ensure a regular IDR interval.
//!
//! ```
//! ffmpeg -re -f lavfi -i testsrc=size=1280x720:rate=30 \
//!   -c:v libx264 -preset ultrafast -tune zerolatency \
//!   -g 30 -keyint_min 30 \
//!   -bsf:v h264_mp4toannexb -f h264 tcp://0.0.0.0:5000?listen
//! ```
//!
//! Then run this example to connect to it:
//! ```
//! cargo run --bin encoded_video -- --url wss://your.livekit.host --api-key <KEY> --api-secret <SECRET> --room <ROOM> --connect 127.0.0.1:5000
//! ```
//!
//! The example connects to the TCP server, reads Annex-B H264 NALUs, identifies
//! keyframes and SPS/PPS presence, and feeds them to an `EncodedVideoSource`
//! which publishes them as a video track in the specified LiveKit room.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::track::LocalVideoTrack;
use livekit::webrtc::encoded_video_source::native::NativeEncodedVideoSource;
use livekit::webrtc::encoded_video_source::{
    EncodedFrameInfo, KeyFrameRequestCallback, VideoCodecType,
};
use livekit::webrtc::video_source::RtcVideoSource;
use livekit_api::access_token;
use log::debug;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest pre-encoded H264 from TCP into LiveKit")]
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
    #[arg(long, default_value = "encoded-video-test")]
    room: String,

    /// TCP server address to connect to for H264 stream
    #[arg(long, default_value = "127.0.0.1:5000")]
    connect: String,

    /// Video width
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Video height
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Frames per second (used for timestamp generation)
    #[arg(long, default_value_t = 30)]
    fps: u32,
}

/// H264 NALU types relevant for keyframe detection
const NALU_TYPE_SLICE: u8 = 1;
const NALU_TYPE_IDR: u8 = 5;
const NALU_TYPE_SPS: u8 = 7;
const NALU_TYPE_PPS: u8 = 8;

/// Simple Annex-B H264 NALU parser.
///
/// Accumulates data from the TCP stream, splits on 3-byte or 4-byte start
/// codes (`00 00 01` or `00 00 00 01`), and groups NALUs into access units
/// (frames) to feed to the encoded video source.
struct AnnexBParser {
    buffer: Vec<u8>,
}

impl AnnexBParser {
    fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(256 * 1024),
        }
    }

    /// Push raw data from the TCP stream and extract complete NALUs.
    /// Returns a list of (nalu_type, nalu_data_including_start_code) pairs.
    fn push(&mut self, data: &[u8]) -> Vec<(u8, Vec<u8>)> {
        self.buffer.extend_from_slice(data);
        let mut nalus = Vec::new();
        let mut start = None;

        let mut i = 0;
        while i < self.buffer.len() {
            // Look for start codes
            let is_4byte_start = i + 3 < self.buffer.len()
                && self.buffer[i] == 0
                && self.buffer[i + 1] == 0
                && self.buffer[i + 2] == 0
                && self.buffer[i + 3] == 1;
            let is_3byte_start = !is_4byte_start
                && i + 2 < self.buffer.len()
                && self.buffer[i] == 0
                && self.buffer[i + 1] == 0
                && self.buffer[i + 2] == 1;

            if is_4byte_start || is_3byte_start {
                if let Some(prev_start) = start {
                    let nalu_data = self.buffer[prev_start..i].to_vec();
                    if !nalu_data.is_empty() {
                        let nalu_type = find_nalu_type(&nalu_data);
                        nalus.push((nalu_type, nalu_data));
                    }
                }
                start = Some(i);
                i += if is_4byte_start { 4 } else { 3 };
            } else {
                i += 1;
            }
        }

        // Keep the remaining partial NALU in the buffer
        if let Some(prev_start) = start {
            self.buffer = self.buffer[prev_start..].to_vec();
        }

        nalus
    }
}

/// Extract the NALU type from Annex-B data (skips start code prefix).
fn find_nalu_type(nalu_with_start: &[u8]) -> u8 {
    // Skip 00 00 01 or 00 00 00 01
    let offset = if nalu_with_start.len() > 3
        && nalu_with_start[0] == 0
        && nalu_with_start[1] == 0
        && nalu_with_start[2] == 0
        && nalu_with_start[3] == 1
    {
        4
    } else if nalu_with_start.len() > 2
        && nalu_with_start[0] == 0
        && nalu_with_start[1] == 0
        && nalu_with_start[2] == 1
    {
        3
    } else {
        return 0;
    };

    if offset < nalu_with_start.len() {
        nalu_with_start[offset] & 0x1f
    } else {
        0
    }
}

/// Group NALUs into a single access unit (frame) by accumulating all NALUs
/// until we hit the next VCL NALU or end of input.
///
/// An H264 access unit typically looks like:
///   [SPS] [PPS] [IDR]        -- keyframe
///   [slice]                   -- delta frame
///
/// The tricky part is that non-VCL NALUs (SPS, PPS, SEI, etc.) that appear
/// *between* two VCL NALUs belong to the *next* access unit, not the current
/// one.  For example the stream may look like:
///
///   ... [delta_slice] [SPS] [PPS] [IDR] [delta_slice] ...
///
/// When the IDR arrives we must flush only `[delta_slice]` as the previous
/// frame, keeping `[SPS] [PPS]` in the buffer so they get grouped with the
/// IDR.
///
/// Additionally, many H264 encoders send SPS/PPS only once at the start of
/// the stream (or periodically as standalone non-VCL access units), separate
/// from the IDR they logically belong to. To handle this, we cache the most
/// recently seen SPS and PPS NALUs and prepend them to any keyframe that
/// does not already include them. Without SPS/PPS, the decoder cannot
/// initialise even if it receives a valid IDR.
struct FrameAssembler {
    pending_nalus: Vec<(u8, Vec<u8>)>,
    /// Whether we have seen at least one keyframe. We must drop all frames
    /// until the first keyframe because the decoder cannot start without one.
    seen_keyframe: bool,
    /// Cached SPS NALU (including Annex-B start code) for prepending to IDRs.
    cached_sps: Option<Vec<u8>>,
    /// Cached PPS NALU (including Annex-B start code) for prepending to IDRs.
    cached_pps: Option<Vec<u8>>,
}

impl FrameAssembler {
    fn new() -> Self {
        Self {
            pending_nalus: Vec::new(),
            seen_keyframe: false,
            cached_sps: None,
            cached_pps: None,
        }
    }

    /// Feed NALUs and return complete frames as byte vectors.
    /// Each returned Vec<u8> contains the Annex-B data for one complete
    /// access unit (frame).
    ///
    /// Frames before the first keyframe (with SPS/PPS) are silently dropped
    /// because WebRTC and downstream decoders require an IDR with SPS/PPS to
    /// initialise.
    /// Returns `(emitted_frames, dropped_count)`.
    fn push_nalus(&mut self, nalus: Vec<(u8, Vec<u8>)>) -> (Vec<FrameData>, u64) {
        let mut frames = Vec::new();
        let mut dropped: u64 = 0;

        for (nalu_type, nalu_data) in nalus {
            // Cache every SPS/PPS we see, even from dropped frames.
            match nalu_type {
                NALU_TYPE_SPS => {
                    self.cached_sps = Some(nalu_data.clone());
                }
                NALU_TYPE_PPS => {
                    self.cached_pps = Some(nalu_data.clone());
                }
                _ => {}
            }

            let is_vcl = matches!(nalu_type, NALU_TYPE_SLICE..=NALU_TYPE_IDR);

            if is_vcl && !self.pending_nalus.is_empty() {
                // Check if previous pending had a VCL -- if so, flush as frame
                let has_prev_vcl = self
                    .pending_nalus
                    .iter()
                    .any(|(t, _)| matches!(t, NALU_TYPE_SLICE..=NALU_TYPE_IDR));
                if has_prev_vcl {
                    let mut frame = self.flush_frame_before_next_au();

                    // If this is a keyframe without SPS/PPS, prepend cached ones
                    if frame.is_keyframe && !frame.has_sps_pps {
                        self.prepend_cached_sps_pps(&mut frame);
                    }

                    // Only emit frames starting from the first keyframe that
                    // has SPS/PPS (either inline or injected from cache).
                    if frame.is_keyframe && frame.has_sps_pps {
                        if !self.seen_keyframe {
                            println!(
                                "First keyframe found! (size={} bytes, nalus={})",
                                frame.data.len(),
                                frame.nalu_count
                            );
                        }
                        self.seen_keyframe = true;
                    }
                    if self.seen_keyframe {
                        frames.push(frame);
                    } else {
                        dropped += 1;
                    }
                }
            }

            self.pending_nalus.push((nalu_type, nalu_data));
        }

        (frames, dropped)
    }

    /// Prepend the cached SPS and PPS NALUs to a frame that is missing them.
    fn prepend_cached_sps_pps(&self, frame: &mut FrameData) {
        let mut prefix = Vec::new();
        let mut extra_nalus = 0u32;
        if let Some(sps) = &self.cached_sps {
            prefix.extend_from_slice(sps);
            extra_nalus += 1;
        }
        if let Some(pps) = &self.cached_pps {
            prefix.extend_from_slice(pps);
            extra_nalus += 1;
        }
        if !prefix.is_empty() {
            prefix.extend_from_slice(&frame.data);
            frame.data = prefix;
            frame.has_sps_pps = true;
            frame.nalu_count += extra_nalus as usize;
        }
    }

    /// Flush the pending buffer as one frame, but split off any trailing
    /// non-VCL NALUs (SPS, PPS, SEI, AUD, etc.) that follow the last VCL
    /// NALU — those belong to the *next* access unit.
    fn flush_frame_before_next_au(&mut self) -> FrameData {
        let all = std::mem::take(&mut self.pending_nalus);

        // Find the index of the last VCL NALU in the buffer.
        let last_vcl_idx = all
            .iter()
            .rposition(|(t, _)| matches!(t, NALU_TYPE_SLICE..=NALU_TYPE_IDR));

        // Split: everything up to and including the last VCL is the current
        // frame; anything after it is the start of the next access unit.
        let split_at = match last_vcl_idx {
            Some(idx) => idx + 1,
            None => all.len(), // no VCL at all (shouldn't happen, but be safe)
        };

        let (frame_nalus, carry_over) = all.split_at(split_at);

        // Put the carry-over NALUs back into pending for the next frame
        self.pending_nalus = carry_over.to_vec();

        Self::build_frame(frame_nalus)
    }

    /// Build a FrameData from a slice of NALUs.
    fn build_frame(nalus: &[(u8, Vec<u8>)]) -> FrameData {
        let mut data = Vec::new();
        let mut is_keyframe = false;
        let mut has_sps_pps = false;

        for (nalu_type, nalu_data) in nalus {
            data.extend_from_slice(nalu_data);
            match *nalu_type {
                NALU_TYPE_IDR => is_keyframe = true,
                NALU_TYPE_SPS | NALU_TYPE_PPS => has_sps_pps = true,
                _ => {}
            }
        }

        FrameData {
            data,
            is_keyframe,
            has_sps_pps,
            nalu_count: nalus.len(),
        }
    }

    /// Flush any remaining pending NALUs as a final frame.
    fn flush_remaining(&mut self) -> Option<FrameData> {
        if self.pending_nalus.is_empty() {
            return None;
        }
        let nalus = std::mem::take(&mut self.pending_nalus);
        if self.seen_keyframe {
            Some(Self::build_frame(&nalus))
        } else {
            None
        }
    }
}

struct FrameData {
    data: Vec<u8>,
    is_keyframe: bool,
    has_sps_pps: bool,
    nalu_count: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    println!(
        "Starting encoded video ingest: {}x{} @ {}fps",
        args.width, args.height, args.fps
    );

    // Create the encoded video source
    let mut encoded_source =
        NativeEncodedVideoSource::new(args.width, args.height, VideoCodecType::H264);

    // Register a keyframe request callback so that when WebRTC needs a keyframe
    // (e.g. on subscriber join or packet loss), we know about it.
    // In a real application you would signal the upstream encoder to produce an IDR.
    struct KfCallback;
    impl KeyFrameRequestCallback for KfCallback {
        fn on_keyframe_request(&self) {
            println!("WebRTC requested a keyframe (PLI)");
        }
    }
    encoded_source.set_keyframe_request_callback(Arc::new(KfCallback));

    let rtc_source = RtcVideoSource::Encoded(encoded_source.clone());

    // Create a video track from it
    let video_track = LocalVideoTrack::create_video_track("h264-ingest", rtc_source);

    // Generate access token
    let token = access_token::AccessToken::with_api_key(&args.api_key, &args.api_secret)
        .with_identity("encoded-video-publisher")
        .with_name("Encoded Video Publisher")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room.clone(),
            ..Default::default()
        })
        .to_jwt()?;

    // Connect to room
    println!("Connecting to room: {}", args.room);
    let (room, mut events) = Room::connect(&args.url, &token, RoomOptions::default()).await?;
    println!("Connected to room: {}", args.room);

    // Publish the video track
    let publish_options = TrackPublishOptions {
        simulcast: false, // no simulcast for passthrough
        source: TrackSource::Camera,
        ..Default::default()
    };
    println!("Publishing video track...");
    let publication = room
        .local_participant()
        .publish_track(
            livekit::track::LocalTrack::Video(video_track),
            publish_options,
        )
        .await?;
    println!("Published video track: {}", publication.sid());

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        println!("Shutting down...");
        running_clone.store(false, Ordering::SeqCst);
    });

    // Spawn room event handler
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            debug!("Room event: {:?}", event);
        }
    });

    // Connect to TCP server for H264 stream
    println!("Connecting to TCP server at {}...", args.connect);
    let mut socket = TcpStream::connect(&args.connect).await?;
    println!("Connected to TCP server at {}", args.connect);

    let mut parser = AnnexBParser::new();
    let mut assembler = FrameAssembler::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut frame_count: u64 = 0;
    let mut keyframe_count: u64 = 0;
    let mut dropped_count: u64 = 0;
    let mut bytes_received: u64 = 0;

    // Use wall-clock time for capture timestamps so WebRTC sees realistic
    // inter-frame intervals.  capture_time_us=0 tells the C++ side to use
    // rtc::TimeMicros(), but being explicit is better for jitter-buffer
    // behaviour on the receiver.
    let start_time = Instant::now();

    println!("Reading H264 stream (waiting for first keyframe)...");

    loop {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        match socket.read(&mut buf).await {
            Ok(0) => {
                println!("TCP server closed connection");
                // Flush remaining
                if let Some(frame) = assembler.flush_remaining() {
                    let capture_time_us = start_time.elapsed().as_micros() as i64;
                    let info = EncodedFrameInfo {
                        data: frame.data,
                        capture_time_us,
                        rtp_timestamp: 0,
                        width: args.width,
                        height: args.height,
                        is_keyframe: frame.is_keyframe,
                        has_sps_pps: frame.has_sps_pps,
                    };
                    encoded_source.capture_frame(&info);
                    frame_count += 1;
                }
                break;
            }
            Ok(n) => {
                bytes_received += n as u64;
                let nalus = parser.push(&buf[..n]);
                let (frames, dropped) = assembler.push_nalus(nalus);
                dropped_count += dropped;

                for frame in frames {
                    let capture_time_us = start_time.elapsed().as_micros() as i64;
                    let is_keyframe = frame.is_keyframe;
                    let has_sps_pps = frame.has_sps_pps;
                    let frame_size = frame.data.len();
                    let nalu_count = frame.nalu_count;

                    if is_keyframe {
                        keyframe_count += 1;
                    }

                    let info = EncodedFrameInfo {
                        data: frame.data,
                        capture_time_us,
                        rtp_timestamp: 0,
                        width: args.width,
                        height: args.height,
                        is_keyframe,
                        has_sps_pps,
                    };

                    let ok = encoded_source.capture_frame(&info);
                    frame_count += 1;

                    if frame_count <= 5 || frame_count % 100 == 0 || is_keyframe {
                        println!(
                            "Frame #{}: size={} bytes, keyframe={}, sps_pps={}, nalus={}, capture_ok={}, total_bytes={}",
                            frame_count, frame_size, is_keyframe, has_sps_pps, nalu_count, ok, bytes_received
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("TCP read error: {}", e);
                break;
            }
        }
    }

    println!(
        "Done. Total frames: {}, keyframes: {}, dropped_before_first_kf: {}, bytes received: {}",
        frame_count, keyframe_count, dropped_count, bytes_received
    );
    room.close().await?;
    println!("Room closed");
    Ok(())
}
