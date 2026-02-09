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

//! Example: Ingest pre-encoded H264/H265 video from a TCP server or file into a LiveKit room.
//!
//! # Usage
//!
//! ## H264 from TCP (default, single layer)
//!
//! First start a TCP server that streams Annex-B H264 data (e.g. with ffmpeg).
//!
//! **Important**: use `-g 30` (or similar) so that keyframes are emitted
//! frequently enough for subscribers to start receiving video quickly.
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
//! ## H265 from TCP
//!
//! ```
//! ffmpeg -re -f lavfi -i testsrc=size=1280x720:rate=30 \
//!   -c:v libx265 -preset ultrafast -tune zerolatency \
//!   -g 30 -keyint_min 30 \
//!   -f hevc tcp://0.0.0.0:5000?listen
//! ```
//!
//! ```
//! cargo run --bin encoded_video -- --codec h265 --connect 127.0.0.1:5000 \
//!   --url wss://your.livekit.host --api-key <KEY> --api-secret <SECRET> --room <ROOM>
//! ```
//!
//! ## From a file
//!
//! Generate an Annex-B file:
//! ```
//! ffmpeg -f lavfi -i testsrc=size=1280x720:rate=30:duration=10 \
//!   -c:v libx264 -preset ultrafast -g 30 -keyint_min 30 \
//!   -bsf:v h264_mp4toannexb -f h264 test.h264
//! ```
//!
//! Play it into a room:
//! ```
//! cargo run --bin encoded_video -- --file test.h264 --codec h264 \
//!   --url wss://your.livekit.host --api-key <KEY> --api-secret <SECRET> --room <ROOM>
//! ```
//!
//! Use `--loop-file` to replay the file continuously.
//!
//! ## Simulcast (3-layer) from TCP
//!
//! Start three ffmpeg instances, each encoding at a different resolution:
//!
//! ```
//! # Low quality (q) — 320x180
//! ffmpeg -re -f lavfi -i testsrc=size=320x180:rate=15 \
//!   -c:v libx264 -preset ultrafast -tune zerolatency \
//!   -b:v 150k -g 30 -keyint_min 30 \
//!   -bsf:v h264_mp4toannexb -f h264 tcp://0.0.0.0:5000?listen
//!
//! # Medium quality (h) — 640x360
//! ffmpeg -re -f lavfi -i testsrc=size=640x360:rate=20 \
//!   -c:v libx264 -preset ultrafast -tune zerolatency \
//!   -b:v 500k -g 30 -keyint_min 30 \
//!   -bsf:v h264_mp4toannexb -f h264 tcp://0.0.0.0:5001?listen
//!
//! # High quality (f) — 1280x720
//! ffmpeg -re -f lavfi -i testsrc=size=1280x720:rate=30 \
//!   -c:v libx264 -preset ultrafast -tune zerolatency \
//!   -b:v 1700k -g 30 -keyint_min 30 \
//!   -bsf:v h264_mp4toannexb -f h264 tcp://0.0.0.0:5002?listen
//! ```
//!
//! Then run with simulcast args:
//! ```
//! cargo run --bin encoded_video -- \
//!   --url wss://your.livekit.host --api-key <KEY> --api-secret <SECRET> --room <ROOM> \
//!   --connect-q 127.0.0.1:5000 --width-q 320 --height-q 180 --bitrate-q 150000 --fps-q 15 \
//!   --connect-h 127.0.0.1:5001 --width-h 640 --height-h 360 --bitrate-h 500000 --fps-h 20 \
//!   --connect-f 127.0.0.1:5002 --width-f 1280 --height-f 720 --bitrate-f 1700000 --fps-f 30
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Result};
use clap::{Parser, ValueEnum};
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::track::LocalVideoTrack;
use livekit::webrtc::encoded_video_source::native::NativeEncodedVideoSource;
use livekit::webrtc::encoded_video_source::{
    EncodedFrameInfo, KeyFrameRequestCallback, VideoCodecType,
};
use livekit::webrtc::video_source::{
    EncodedSimulcastLayer, RtcVideoSource, SimulcastEncodedVideoSource,
};
use livekit_api::access_token;
use log::debug;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpStream;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CodecArg {
    H264,
    H265,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Ingest pre-encoded H264/H265 video into LiveKit")]
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

    /// TCP server address to connect to for the encoded stream (single layer).
    /// Mutually exclusive with --file and --connect-{q,h,f}.
    #[arg(long)]
    connect: Option<String>,

    /// Path to a local Annex-B .h264/.h265 file.
    /// Mutually exclusive with --connect and --connect-{q,h,f}.
    #[arg(long)]
    file: Option<String>,

    /// Loop the file continuously (only used with --file)
    #[arg(long, default_value_t = false)]
    loop_file: bool,

    /// Video codec
    #[arg(long, value_enum, default_value_t = CodecArg::H264)]
    codec: CodecArg,

    /// Video width (single-layer mode)
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Video height (single-layer mode)
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Frames per second (used for frame-rate pacing when reading from a file)
    #[arg(long, default_value_t = 30)]
    fps: u32,

    // -- Simulcast layer: q (low quality) --
    /// TCP address for the low-quality (q) simulcast layer
    #[arg(long)]
    connect_q: Option<String>,
    /// Width for the q layer
    #[arg(long, default_value_t = 320)]
    width_q: u32,
    /// Height for the q layer
    #[arg(long, default_value_t = 180)]
    height_q: u32,
    /// Max bitrate (bps) for the q layer
    #[arg(long, default_value_t = 150_000)]
    bitrate_q: u64,
    /// Max framerate for the q layer
    #[arg(long, default_value_t = 15.0)]
    fps_q: f64,

    // -- Simulcast layer: h (medium quality) --
    /// TCP address for the medium-quality (h) simulcast layer
    #[arg(long)]
    connect_h: Option<String>,
    /// Width for the h layer
    #[arg(long, default_value_t = 640)]
    width_h: u32,
    /// Height for the h layer
    #[arg(long, default_value_t = 360)]
    height_h: u32,
    /// Max bitrate (bps) for the h layer
    #[arg(long, default_value_t = 500_000)]
    bitrate_h: u64,
    /// Max framerate for the h layer
    #[arg(long, default_value_t = 20.0)]
    fps_h: f64,

    // -- Simulcast layer: f (high quality) --
    /// TCP address for the high-quality (f) simulcast layer
    #[arg(long)]
    connect_f: Option<String>,
    /// Width for the f layer
    #[arg(long, default_value_t = 1280)]
    width_f: u32,
    /// Height for the f layer
    #[arg(long, default_value_t = 720)]
    height_f: u32,
    /// Max bitrate (bps) for the f layer
    #[arg(long, default_value_t = 1_700_000)]
    bitrate_f: u64,
    /// Max framerate for the f layer
    #[arg(long, default_value_t = 30.0)]
    fps_f: f64,
}

impl Args {
    /// Returns true if any simulcast layer TCP address is specified.
    fn is_simulcast(&self) -> bool {
        self.connect_q.is_some() || self.connect_h.is_some() || self.connect_f.is_some()
    }
}

// ---------------------------------------------------------------------------
// Codec-aware NALU helpers
// ---------------------------------------------------------------------------

/// Which codec we are parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codec {
    H264,
    H265,
}

// -- H264 NALU types --
const H264_NALU_SLICE: u8 = 1;
const H264_NALU_IDR: u8 = 5;
const H264_NALU_SPS: u8 = 7;
const H264_NALU_PPS: u8 = 8;

// -- H265 (HEVC) NALU types --
const H265_NALU_VPS: u8 = 32;
const H265_NALU_SPS: u8 = 33;
const H265_NALU_PPS: u8 = 34;
const H265_NALU_IDR_W_RADL: u8 = 19;
const H265_NALU_IDR_N_LP: u8 = 20;

/// Extract the NALU type byte from Annex-B data (after the start code).
fn find_nalu_type(nalu_with_start: &[u8], codec: Codec) -> u8 {
    // Skip 00 00 00 01 or 00 00 01
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

    if offset >= nalu_with_start.len() {
        return 0;
    }

    match codec {
        // H264: 1-byte header, type in lower 5 bits
        Codec::H264 => nalu_with_start[offset] & 0x1F,
        // H265: 2-byte header, type in bits [1..6] of the first byte
        Codec::H265 => (nalu_with_start[offset] >> 1) & 0x3F,
    }
}

/// Is this NALU type a VCL (Video Coding Layer) unit — i.e. actual picture data?
fn is_vcl_nalu(nalu_type: u8, codec: Codec) -> bool {
    match codec {
        // H264 VCL types: 1 (coded slice) through 5 (IDR)
        Codec::H264 => matches!(nalu_type, H264_NALU_SLICE..=H264_NALU_IDR),
        // H265 VCL types: 0..=31 (all types < 32 are VCL in HEVC)
        Codec::H265 => nalu_type <= 31,
    }
}

/// Is this NALU type a keyframe (IDR)?
fn is_keyframe_nalu(nalu_type: u8, codec: Codec) -> bool {
    match codec {
        Codec::H264 => nalu_type == H264_NALU_IDR,
        Codec::H265 => matches!(nalu_type, H265_NALU_IDR_W_RADL | H265_NALU_IDR_N_LP),
    }
}

/// Is this a parameter set NALU? (SPS/PPS for H264, VPS/SPS/PPS for H265)
fn is_parameter_set_nalu(nalu_type: u8, codec: Codec) -> bool {
    match codec {
        Codec::H264 => matches!(nalu_type, H264_NALU_SPS | H264_NALU_PPS),
        Codec::H265 => matches!(nalu_type, H265_NALU_VPS | H265_NALU_SPS | H265_NALU_PPS),
    }
}

/// For H265: check `first_slice_segment_in_pic_flag` to detect access-unit
/// boundaries in multi-slice pictures.
///
/// In HEVC a single picture (IDR or otherwise) can be split across multiple
/// VCL NALUs (slices). The first bit of the slice segment header
/// (`first_slice_segment_in_pic_flag`) is 1 only for the first slice of a
/// new picture. Subsequent slices of the *same* picture have it set to 0 and
/// must be grouped into the same access unit.
///
/// `nalu_data` is the full NALU *including* the Annex-B start code prefix.
///
/// Returns `(is_first_slice, parsed_ok)`.
fn h265_first_slice_in_pic(nalu_data: &[u8]) -> (bool, bool) {
    // Skip past the start code to reach the 2-byte NAL header + payload.
    let offset = if nalu_data.len() > 3
        && nalu_data[0] == 0
        && nalu_data[1] == 0
        && nalu_data[2] == 0
        && nalu_data[3] == 1
    {
        4
    } else if nalu_data.len() > 2
        && nalu_data[0] == 0
        && nalu_data[1] == 0
        && nalu_data[2] == 1
    {
        3
    } else {
        return (true, false);
    };

    // Need at least 2-byte NAL header + 1 byte of slice header
    if nalu_data.len() < offset + 3 {
        return (true, false);
    }

    // Byte at offset+2 is the first byte of the slice segment header.
    // Bit 7 (MSB) is first_slice_segment_in_pic_flag.
    let first_flag = (nalu_data[offset + 2] & 0x80) != 0;
    (first_flag, true)
}

// ---------------------------------------------------------------------------
// Annex-B NALU parser (codec-agnostic start-code scanning)
// ---------------------------------------------------------------------------

/// Simple Annex-B NALU parser.
///
/// Accumulates data from the input, splits on 3-byte or 4-byte start
/// codes (`00 00 01` or `00 00 00 01`), and returns individual NALUs
/// with their type tag extracted using the appropriate codec rules.
struct AnnexBParser {
    buffer: Vec<u8>,
    codec: Codec,
}

impl AnnexBParser {
    fn new(codec: Codec) -> Self {
        Self {
            buffer: Vec::with_capacity(256 * 1024),
            codec,
        }
    }

    /// Push raw data and extract complete NALUs.
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
                        let nalu_type = find_nalu_type(&nalu_data, self.codec);
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

// ---------------------------------------------------------------------------
// Frame assembler (groups NALUs into access units)
// ---------------------------------------------------------------------------

/// Group NALUs into access units (frames).
///
/// An access unit typically looks like:
///   H264: [SPS] [PPS] [IDR]        -- keyframe
///         [slice]                   -- delta frame
///   H265: [VPS] [SPS] [PPS] [IDR]  -- keyframe
///         [slice]                   -- delta frame
///
/// Non-VCL NALUs that appear *between* two VCL NALUs belong to the *next*
/// access unit.  We also cache parameter sets (SPS/PPS, and VPS for H265)
/// and prepend them to any keyframe that does not already include them.
struct FrameAssembler {
    codec: Codec,
    pending_nalus: Vec<(u8, Vec<u8>)>,
    /// Whether we have seen at least one keyframe.
    seen_keyframe: bool,
    /// Cached VPS NALU (H265 only).
    cached_vps: Option<Vec<u8>>,
    /// Cached SPS NALU.
    cached_sps: Option<Vec<u8>>,
    /// Cached PPS NALU.
    cached_pps: Option<Vec<u8>>,
}

impl FrameAssembler {
    fn new(codec: Codec) -> Self {
        Self {
            codec,
            pending_nalus: Vec::new(),
            seen_keyframe: false,
            cached_vps: None,
            cached_sps: None,
            cached_pps: None,
        }
    }

    /// Feed NALUs and return complete frames.
    ///
    /// Frames before the first keyframe (with parameter sets) are silently
    /// dropped because decoders require an IDR with parameter sets to start.
    /// Returns `(emitted_frames, dropped_count)`.
    fn push_nalus(&mut self, nalus: Vec<(u8, Vec<u8>)>) -> (Vec<FrameData>, u64) {
        let mut frames = Vec::new();
        let mut dropped: u64 = 0;
        let codec = self.codec;

        for (nalu_type, nalu_data) in nalus {
            // Cache every parameter set we see, even from dropped frames.
            self.cache_parameter_set(nalu_type, &nalu_data);

            let is_vcl = is_vcl_nalu(nalu_type, codec);

            if is_vcl && !self.pending_nalus.is_empty() {
                // Check if previous pending had a VCL -- if so, this *might*
                // be the start of a new access unit (= new frame).
                let has_prev_vcl = self
                    .pending_nalus
                    .iter()
                    .any(|(t, _)| is_vcl_nalu(*t, codec));

                if has_prev_vcl {
                    // For H265, a single picture (IDR or otherwise) can be
                    // split into multiple VCL NALUs (slices).  We must check
                    // first_slice_segment_in_pic_flag to know whether this
                    // VCL NALU starts a *new* picture or is a continuation
                    // slice of the current one.
                    let starts_new_au = match codec {
                        Codec::H264 => true, // H264 single-slice assumption
                        Codec::H265 => {
                            let (is_first, ok) = h265_first_slice_in_pic(&nalu_data);
                            // If we can't parse the flag, err on the side of
                            // splitting (same behaviour as the Go SDK).
                            !ok || is_first
                        }
                    };

                    if starts_new_au {
                        let mut frame = self.flush_frame_before_next_au();

                        // If this is a keyframe without parameter sets, prepend cached ones
                        if frame.is_keyframe && !frame.has_parameter_sets {
                            self.prepend_cached_parameter_sets(&mut frame);
                        }

                        // Only emit frames starting from the first keyframe that
                        // has parameter sets (either inline or injected from cache).
                        if frame.is_keyframe && frame.has_parameter_sets {
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
                    // If !starts_new_au, this is a continuation slice of the
                    // same picture — just fall through and append it to pending.
                }
            }

            self.pending_nalus.push((nalu_type, nalu_data));
        }

        (frames, dropped)
    }

    /// Cache a parameter set NALU if it matches the codec's parameter set types.
    fn cache_parameter_set(&mut self, nalu_type: u8, nalu_data: &[u8]) {
        match self.codec {
            Codec::H264 => match nalu_type {
                H264_NALU_SPS => self.cached_sps = Some(nalu_data.to_vec()),
                H264_NALU_PPS => self.cached_pps = Some(nalu_data.to_vec()),
                _ => {}
            },
            Codec::H265 => match nalu_type {
                H265_NALU_VPS => self.cached_vps = Some(nalu_data.to_vec()),
                H265_NALU_SPS => self.cached_sps = Some(nalu_data.to_vec()),
                H265_NALU_PPS => self.cached_pps = Some(nalu_data.to_vec()),
                _ => {}
            },
        }
    }

    /// Prepend cached parameter sets to a frame that is missing them.
    fn prepend_cached_parameter_sets(&self, frame: &mut FrameData) {
        let mut prefix = Vec::new();
        let mut extra_nalus = 0usize;

        // H265 needs VPS before SPS/PPS
        if self.codec == Codec::H265 {
            if let Some(vps) = &self.cached_vps {
                prefix.extend_from_slice(vps);
                extra_nalus += 1;
            }
        }
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
            frame.has_parameter_sets = true;
            frame.nalu_count += extra_nalus;
        }
    }

    /// Flush the pending buffer as one frame, splitting off any trailing
    /// non-VCL NALUs that belong to the *next* access unit.
    fn flush_frame_before_next_au(&mut self) -> FrameData {
        let all = std::mem::take(&mut self.pending_nalus);
        let codec = self.codec;

        // Find the index of the last VCL NALU in the buffer.
        let last_vcl_idx = all.iter().rposition(|(t, _)| is_vcl_nalu(*t, codec));

        let split_at = match last_vcl_idx {
            Some(idx) => idx + 1,
            None => all.len(),
        };

        let (frame_nalus, carry_over) = all.split_at(split_at);
        self.pending_nalus = carry_over.to_vec();

        Self::build_frame(frame_nalus, codec)
    }

    /// Build a FrameData from a slice of NALUs.
    fn build_frame(nalus: &[(u8, Vec<u8>)], codec: Codec) -> FrameData {
        let mut data = Vec::new();
        let mut is_keyframe = false;
        let mut has_parameter_sets = false;

        for (nalu_type, nalu_data) in nalus {
            data.extend_from_slice(nalu_data);
            if is_keyframe_nalu(*nalu_type, codec) {
                is_keyframe = true;
            }
            if is_parameter_set_nalu(*nalu_type, codec) {
                has_parameter_sets = true;
            }
        }

        FrameData {
            data,
            is_keyframe,
            has_parameter_sets,
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
            Some(Self::build_frame(&nalus, self.codec))
        } else {
            None
        }
    }
}

struct FrameData {
    data: Vec<u8>,
    is_keyframe: bool,
    has_parameter_sets: bool,
    nalu_count: usize,
}

// ---------------------------------------------------------------------------
// TCP layer reader task (shared by single-layer and simulcast modes)
// ---------------------------------------------------------------------------

/// Run a TCP ingest loop for a single encoded video source layer.
///
/// Reads Annex-B data from `addr`, parses NALUs, assembles frames, and
/// captures them into `encoded_source` with the given `simulcast_index`.
async fn run_tcp_layer(
    layer_name: &str,
    addr: &str,
    codec: Codec,
    width: u32,
    height: u32,
    simulcast_index: u32,
    encoded_source: NativeEncodedVideoSource,
    running: Arc<AtomicBool>,
) {
    println!("[{layer_name}] Connecting to TCP server at {addr}...");
    let stream = match TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[{layer_name}] Failed to connect to {addr}: {e}");
            return;
        }
    };
    println!("[{layer_name}] Connected to TCP server at {addr}");

    let mut parser = AnnexBParser::new(codec);
    let mut assembler = FrameAssembler::new(codec);
    let mut buf = vec![0u8; 64 * 1024];
    let mut frame_count: u64 = 0;
    let mut keyframe_count: u64 = 0;
    let mut dropped_count: u64 = 0;
    let mut bytes_received: u64 = 0;
    let start_time = Instant::now();

    let mut source: Box<dyn AsyncRead + Unpin> = Box::new(stream);

    println!("[{layer_name}] Reading stream (waiting for first keyframe)...");

    loop {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        match source.read(&mut buf).await {
            Ok(0) => {
                println!("[{layer_name}] TCP server closed connection");
                if let Some(frame) = assembler.flush_remaining() {
                    let capture_time_us = start_time.elapsed().as_micros() as i64;
                    let info = EncodedFrameInfo {
                        data: frame.data,
                        capture_time_us,
                        rtp_timestamp: 0,
                        width,
                        height,
                        is_keyframe: frame.is_keyframe,
                        has_sps_pps: frame.has_parameter_sets,
                        simulcast_index,
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
                    let has_param_sets = frame.has_parameter_sets;
                    let frame_size = frame.data.len();
                    let nalu_count = frame.nalu_count;

                    if is_keyframe {
                        keyframe_count += 1;
                    }

                    let info = EncodedFrameInfo {
                        data: frame.data,
                        capture_time_us,
                        rtp_timestamp: 0,
                        width,
                        height,
                        is_keyframe,
                        has_sps_pps: has_param_sets,
                        simulcast_index,
                    };

                    let ok = encoded_source.capture_frame(&info);
                    frame_count += 1;

                    if frame_count <= 3 || frame_count % 100 == 0 || is_keyframe {
                        println!(
                            "[{layer_name}] Frame #{}: size={} bytes, kf={}, ps={}, nalus={}, ok={}, total_bytes={}",
                            frame_count, frame_size, is_keyframe, has_param_sets, nalu_count, ok, bytes_received
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("[{layer_name}] Read error: {e}");
                break;
            }
        }
    }

    println!(
        "[{layer_name}] Done. frames={}, keyframes={}, dropped={}, bytes={}",
        frame_count, keyframe_count, dropped_count, bytes_received
    );
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let is_simulcast = args.is_simulcast();

    // Validate input source
    if !is_simulcast && args.connect.is_none() && args.file.is_none() {
        bail!("Either --connect, --file, or simulcast --connect-{{q,h,f}} must be specified");
    }
    if !is_simulcast && args.connect.is_some() && args.file.is_some() {
        bail!("--connect and --file are mutually exclusive");
    }
    if is_simulcast && (args.connect.is_some() || args.file.is_some()) {
        bail!("--connect-{{q,h,f}} cannot be combined with --connect or --file");
    }

    let codec = match args.codec {
        CodecArg::H264 => Codec::H264,
        CodecArg::H265 => Codec::H265,
    };
    let codec_type = match codec {
        Codec::H264 => VideoCodecType::H264,
        Codec::H265 => VideoCodecType::H265,
    };
    let video_codec = match codec {
        Codec::H264 => livekit::options::VideoCodec::H264,
        Codec::H265 => livekit::options::VideoCodec::H265,
    };
    let codec_name = match codec {
        Codec::H264 => "H264",
        Codec::H265 => "H265",
    };

    // -- Build the video source (single-layer or simulcast) --

    struct KfCallback {
        label: String,
    }
    impl KeyFrameRequestCallback for KfCallback {
        fn on_keyframe_request(&self) {
            println!("[{}] WebRTC requested a keyframe (PLI)", self.label);
        }
    }

    let rtc_source: RtcVideoSource;

    if is_simulcast {
        // Collect the specified layers (at least 2 required for simulcast)
        let mut layers: Vec<(String, String, u32, u32, u64, f64)> = Vec::new();
        if let Some(ref addr) = args.connect_q {
            layers.push((
                "q".into(),
                addr.clone(),
                args.width_q,
                args.height_q,
                args.bitrate_q,
                args.fps_q,
            ));
        }
        if let Some(ref addr) = args.connect_h {
            layers.push((
                "h".into(),
                addr.clone(),
                args.width_h,
                args.height_h,
                args.bitrate_h,
                args.fps_h,
            ));
        }
        if let Some(ref addr) = args.connect_f {
            layers.push((
                "f".into(),
                addr.clone(),
                args.width_f,
                args.height_f,
                args.bitrate_f,
                args.fps_f,
            ));
        }
        if layers.len() < 2 {
            bail!("Simulcast requires at least 2 layers (specify at least 2 of --connect-q, --connect-h, --connect-f)");
        }

        println!(
            "Starting simulcast encoded video ingest ({codec_name}): {} layers",
            layers.len()
        );

        let mut sim_layers = Vec::new();
        for (name, _addr, w, h, bitrate, fps) in &layers {
            println!("  Layer {name}: {w}x{h} @ {fps}fps, {bitrate}bps");
            let mut source = NativeEncodedVideoSource::new(*w, *h, codec_type);
            source.set_keyframe_request_callback(Arc::new(KfCallback {
                label: format!("layer-{name}"),
            }));
            sim_layers.push(EncodedSimulcastLayer {
                source,
                width: *w,
                height: *h,
                max_bitrate: *bitrate,
                max_framerate: *fps,
            });
        }

        let simulcast_source = SimulcastEncodedVideoSource::new(sim_layers);
        rtc_source = RtcVideoSource::SimulcastEncoded(simulcast_source);
    } else {
        println!(
            "Starting encoded video ingest ({codec_name}): {}x{} @ {}fps",
            args.width, args.height, args.fps
        );

        let mut encoded_source = NativeEncodedVideoSource::new(args.width, args.height, codec_type);
        encoded_source.set_keyframe_request_callback(Arc::new(KfCallback {
            label: "single".into(),
        }));
        rtc_source = RtcVideoSource::Encoded(encoded_source);
    }

    // Create a video track from it
    let track_name = format!("{}-ingest", codec_name.to_lowercase());
    let video_track = LocalVideoTrack::create_video_track(&track_name, rtc_source.clone());

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
        source: TrackSource::Camera,
        video_codec,
        ..Default::default()
    };
    println!("Publishing video track ({codec_name}, simulcast={is_simulcast})...");
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

    if is_simulcast {
        // -- Simulcast mode: spawn one TCP reader task per layer --
        let simulcast_source = match &rtc_source {
            RtcVideoSource::SimulcastEncoded(s) => s,
            _ => unreachable!(),
        };

        let mut handles = Vec::new();

        // Re-collect layer info to pair with sources
        let mut layer_info: Vec<(String, String, u32, u32)> = Vec::new();
        if let Some(ref addr) = args.connect_q {
            layer_info.push(("q".into(), addr.clone(), args.width_q, args.height_q));
        }
        if let Some(ref addr) = args.connect_h {
            layer_info.push(("h".into(), addr.clone(), args.width_h, args.height_h));
        }
        if let Some(ref addr) = args.connect_f {
            layer_info.push(("f".into(), addr.clone(), args.width_f, args.height_f));
        }

        for (i, (name, addr, w, h)) in layer_info.into_iter().enumerate() {
            let layer_source = simulcast_source.layers()[i].source.clone();
            let running = running.clone();
            let codec = codec;
            let simulcast_index = i as u32;
            handles.push(tokio::spawn(async move {
                run_tcp_layer(
                    &name,
                    &addr,
                    codec,
                    w,
                    h,
                    simulcast_index,
                    layer_source,
                    running,
                )
                .await;
            }));
        }

        // Wait for all layer tasks to complete
        for handle in handles {
            let _ = handle.await;
        }
    } else {
        // -- Single-layer mode (TCP or file) --
        let encoded_source = match &rtc_source {
            RtcVideoSource::Encoded(s) => s.clone(),
            _ => unreachable!(),
        };

        // Frame pacing interval (used for file input to simulate real-time)
        let frame_interval = std::time::Duration::from_secs_f64(1.0 / args.fps as f64);

        // Run the ingest loop — may iterate more than once when --loop-file is set.
        loop {
            // Open the input source
            let mut source: Box<dyn AsyncRead + Unpin> = if let Some(ref addr) = args.connect {
                println!("Connecting to TCP server at {addr}...");
                let stream = TcpStream::connect(addr).await?;
                println!("Connected to TCP server at {addr}");
                Box::new(stream)
            } else {
                let path = args.file.as_ref().unwrap();
                println!("Opening file: {path}");
                let file = tokio::fs::File::open(path).await?;
                Box::new(file)
            };

            let mut parser = AnnexBParser::new(codec);
            let mut assembler = FrameAssembler::new(codec);
            let mut buf = vec![0u8; 64 * 1024];
            let mut frame_count: u64 = 0;
            let mut keyframe_count: u64 = 0;
            let mut dropped_count: u64 = 0;
            let mut bytes_received: u64 = 0;

            let start_time = Instant::now();
            let is_file = args.file.is_some();

            println!("Reading {codec_name} stream (waiting for first keyframe)...");

            loop {
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                match source.read(&mut buf).await {
                    Ok(0) => {
                        println!(
                            "{}",
                            if is_file {
                                "End of file reached"
                            } else {
                                "TCP server closed connection"
                            }
                        );
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
                                has_sps_pps: frame.has_parameter_sets,
                                simulcast_index: 0,
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
                            // For file input, pace frames to approximate real-time playback
                            if is_file {
                                let target_time = frame_interval * frame_count as u32;
                                let elapsed = start_time.elapsed();
                                if target_time > elapsed {
                                    tokio::time::sleep(target_time - elapsed).await;
                                }
                            }

                            let capture_time_us = start_time.elapsed().as_micros() as i64;
                            let is_keyframe = frame.is_keyframe;
                            let has_param_sets = frame.has_parameter_sets;
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
                                has_sps_pps: has_param_sets,
                                simulcast_index: 0,
                            };

                            let ok = encoded_source.capture_frame(&info);
                            frame_count += 1;

                            if frame_count <= 5 || frame_count % 100 == 0 || is_keyframe {
                                println!(
                                    "Frame #{}: size={} bytes, keyframe={}, param_sets={}, nalus={}, capture_ok={}, total_bytes={}",
                                    frame_count, frame_size, is_keyframe, has_param_sets, nalu_count, ok, bytes_received
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Read error: {}", e);
                        break;
                    }
                }
            }

            println!(
                "Done. Total frames: {}, keyframes: {}, dropped_before_first_kf: {}, bytes received: {}",
                frame_count, keyframe_count, dropped_count, bytes_received
            );

            // Loop only for file input with --loop-file
            if !(is_file && args.loop_file && running.load(Ordering::SeqCst)) {
                break;
            }
            println!("Looping file...");
        }
    }

    room.close().await?;
    println!("Room closed");
    Ok(())
}
