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

//! Encoded H.264 / H.265 / VP8 / VP9 / AV1 ingest sender.
//!
//! Connects to a gstreamer pipeline as a TCP client and pushes each
//! decoded access unit / frame straight through
//! `NativeEncodedVideoSource::capture_frame`. No software encoding
//! happens on the Rust side — the bytes on the wire are the bytes that
//! get packetized into RTP.
//!
//! Two framings are supported, picked by `--codec`:
//!
//! * **H.264 / H.265**: raw Annex-B bytestream. The sender splits on
//!   AUD NAL boundaries (NAL type 9 for H.264, type 35 for H.265) and
//!   delivers each access unit.
//! * **VP8 / VP9 / AV1**: IVF container (gstreamer's `ivfmux` or
//!   `avmux_ivf`). The sender parses the 32-byte IVF file header once
//!   (when present), then each 12-byte frame header + payload, and
//!   delivers each raw VPx frame (for AV1, each IVF record is one
//!   Temporal Unit — a complete OBU sequence for one frame).
//!
//! TCP is used instead of UDP because macOS caps per-datagram UDP
//! payloads well below 64 KB by default, which is easy to exceed with
//! keyframes. The matching gstreamer pipelines are documented in
//! README.md.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use libwebrtc::video_source::{EncodedFrameInfo, RtcVideoSource, VideoCodec, VideoResolution};
use livekit::{
    options::{TrackPublishOptions, VideoCodec as LkVideoCodec, VideoEncoding},
    prelude::*,
    webrtc::video_source::native::{EncodedVideoSourceObserver, NativeEncodedVideoSource},
};
use livekit_api::access_token;
use log::{info, warn};
use tokio::{io::AsyncReadExt, net::TcpStream, time::sleep};

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

    /// Host of the gstreamer `tcpserversink` producing the Annex-B bytestream
    #[arg(long, default_value = "127.0.0.1")]
    tcp_host: String,

    /// Port of the gstreamer `tcpserversink` producing the Annex-B bytestream
    #[arg(long, default_value_t = 5000)]
    tcp_port: u16,

    /// Declared stream width (px)
    #[arg(long, default_value_t = 640)]
    width: u32,

    /// Declared stream height (px)
    #[arg(long, default_value_t = 480)]
    height: u32,

    /// RTP sender max bitrate advertised to WebRTC, in kbps
    #[arg(long, default_value_t = 2_500)]
    max_bitrate_kbps: u64,

    /// RTP sender max framerate advertised to WebRTC
    #[arg(long, default_value_t = 30.0)]
    max_framerate: f64,

    /// Encoded codec on the wire. Must match the gstreamer pipeline.
    #[arg(long, value_enum, default_value_t = CodecArg::H264)]
    codec: CodecArg,
}

/// Codec selector for the CLI. Drives both framing (Annex-B vs. IVF)
/// and keyframe detection.
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

    fn livekit_codec(self) -> LkVideoCodec {
        match self {
            CodecArg::H264 => LkVideoCodec::H264,
            CodecArg::H265 => LkVideoCodec::H265,
            CodecArg::Vp8 => LkVideoCodec::VP8,
            CodecArg::Vp9 => LkVideoCodec::VP9,
            CodecArg::Av1 => LkVideoCodec::AV1,
        }
    }

    /// NAL unit type from the first byte after a start code.
    /// H.264: lower 5 bits. H.265: bits 1..7.
    fn nal_type(self, first_byte: u8) -> u8 {
        match self {
            CodecArg::H264 => first_byte & 0x1F,
            CodecArg::H265 => (first_byte >> 1) & 0x3F,
            // VPx/AV1 have no NAL units; callers should not reach this.
            CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => 0,
        }
    }

    /// Access-unit delimiter NAL type. 9 (AUD) for H.264, 35 (AUD_NUT)
    /// for H.265. Undefined for IVF-framed codecs.
    fn aud_nal_type(self) -> u8 {
        match self {
            CodecArg::H264 => 9,
            CodecArg::H265 => 35,
            CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => u8::MAX,
        }
    }

    /// Whether a given NAL type is a keyframe NAL.
    /// H.264: IDR slice (5). H.265: any IRAP (BLA/IDR/CRA, 16..=23).
    /// IVF-framed codecs use [`is_keyframe`] directly; this never runs.
    fn is_keyframe_nal(self, nal_type: u8) -> bool {
        match self {
            CodecArg::H264 => nal_type == 5,
            CodecArg::H265 => (16..=23).contains(&nal_type),
            CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            CodecArg::H264 => "H.264",
            CodecArg::H265 => "H.265",
            CodecArg::Vp8 => "VP8",
            CodecArg::Vp9 => "VP9",
            CodecArg::Av1 => "AV1",
        }
    }

    /// IVF FOURCC expected on the wire. Only meaningful for codecs
    /// delivered via `ivfmux` / `avmux_ivf`.
    fn ivf_fourcc(self) -> Option<&'static [u8; 4]> {
        match self {
            CodecArg::Vp8 => Some(b"VP80"),
            CodecArg::Vp9 => Some(b"VP90"),
            CodecArg::Av1 => Some(b"AV01"),
            _ => None,
        }
    }
}

/// Simple observer that logs feedback from the encoder pipeline. Real
/// producers should react here — e.g. nudge their hardware encoder to
/// emit an IDR on `on_keyframe_requested`, or clamp bitrate on
/// `on_target_bitrate`.
struct LoggingObserver {
    last_bitrate_log: Mutex<Option<Instant>>,
    target_bitrate_bps: Arc<AtomicU64>,
}

impl LoggingObserver {
    fn new(target_bitrate_bps: Arc<AtomicU64>) -> Self {
        Self { last_bitrate_log: Mutex::new(None), target_bitrate_bps }
    }
}

impl EncodedVideoSourceObserver for LoggingObserver {
    fn on_keyframe_requested(&self) {
        warn!(
            "keyframe requested by receiver — producer should emit a keyframe on the next frame \
             (in this demo the next keyframe comes when the gstreamer encoder hits its \
             keyframe-interval knob, e.g. x264enc/x265enc key-int-max or vp8enc keyframe-max-dist)"
        );
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        self.target_bitrate_bps.store(bitrate_bps as u64, Ordering::Relaxed);

        // Rate-limit logging to 1 Hz.
        let mut last = self.last_bitrate_log.lock().unwrap();
        let now = Instant::now();
        if last.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1)) {
            *last = Some(now);
            info!("target bitrate update: {} kbps @ {:.1} fps", bitrate_bps / 1000, framerate_fps);
        }
    }
}

/// Higher-level demuxer: hides whether the wire is Annex-B or IVF.
enum Demuxer {
    AnnexB(AuSplitter),
    Ivf(IvfReader),
}

impl Demuxer {
    fn new(codec: CodecArg) -> Self {
        match codec {
            CodecArg::H264 | CodecArg::H265 => Demuxer::AnnexB(AuSplitter::new(codec)),
            CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => Demuxer::Ivf(IvfReader::new(codec)),
        }
    }

    fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        match self {
            Demuxer::AnnexB(s) => s.feed(chunk, out),
            Demuxer::Ivf(r) => r.feed(chunk, out),
        }
    }

    /// True if the demuxer has detected a byte misalignment it can't
    /// recover from without a fresh TCP connection. Only meaningful
    /// for IVF today.
    fn desynced(&self) -> bool {
        match self {
            Demuxer::AnnexB(_) => false,
            Demuxer::Ivf(r) => r.desynced,
        }
    }
}

/// Reads IVF-framed video off the wire and emits one compressed video
/// frame per call to `feed` per available frame. Format per libvpx:
///
/// File header (32 bytes, optional): "DKIF", u16 version, u16
///   header_len, 4-byte FOURCC, u16 width, u16 height, u32 tb_num,
///   u32 tb_den, u32 frame_count, u32 unused.
///
/// Frame header (12 bytes each): u32 frame_size, u64 pts.
///
/// Frame payload: `frame_size` bytes. All integers little-endian.
///
/// The file header is *optional* in our parser: gstreamer's
/// `avmux_ivf` on a non-seekable `tcpserversink` emits only per-frame
/// records (libavformat writes `DKIF` at `write_header` time, but the
/// ffmpeg AVIO wrapper in gst-libav appears to swallow it when the
/// output is non-seekable). We still accept `ivfmux` (native
/// gst-plugins-bad element), which does emit `DKIF`, by parsing the
/// file header if it's the first 4 bytes. Either way, gstreamer's
/// one-buffer-per-packet semantics mean new `tcpserversink` clients
/// land on an IVF record boundary.
///
/// If we ever parse a `frame_size` that exceeds [`MAX_FRAME_BYTES`],
/// we're byte-misaligned (should be rare in practice); the reader
/// flips `desynced=true`, which the main loop reads to force a TCP
/// reconnect and a fresh alignment from the next gstreamer buffer.
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

struct IvfReader {
    codec: CodecArg,
    buf: Vec<u8>,
    /// Set once we've either consumed a 32-byte DKIF header or
    /// decided there isn't one. After this, `buf` is interpreted as
    /// back-to-back 12-byte-header + payload records.
    header_phase_done: bool,
    /// True if a frame_size field was absurd; main loop should
    /// disconnect and reconnect to re-align.
    desynced: bool,
}

impl IvfReader {
    fn new(codec: CodecArg) -> Self {
        Self {
            codec,
            buf: Vec::with_capacity(256 * 1024),
            header_phase_done: false,
            desynced: false,
        }
    }

    fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.extend_from_slice(chunk);

        if !self.header_phase_done {
            // Decide whether the stream starts with a DKIF file header.
            // We need at least 4 bytes to check the magic, and 32 to
            // consume the full header if present.
            if self.buf.len() < 4 {
                return;
            }
            if &self.buf[0..4] == b"DKIF" {
                if self.buf.len() < 32 {
                    return;
                }
                let fourcc = &self.buf[8..12];
                if let Some(expected) = self.codec.ivf_fourcc() {
                    if fourcc != expected {
                        warn!(
                            "IVF: expected FOURCC {:?} for {}, got {:?}",
                            std::str::from_utf8(expected).unwrap_or("?"),
                            self.codec.name(),
                            std::str::from_utf8(fourcc).unwrap_or("?"),
                        );
                    }
                }
                info!(
                    "IVF: file header OK (codec fourcc={})",
                    std::str::from_utf8(fourcc).unwrap_or("?")
                );
                self.buf.drain(..32);
            } else {
                // No file header — typical for gstreamer's `avmux_ivf`
                // on tcpserversink. Gstreamer buffer boundaries keep
                // us frame-aligned, so treat byte 0 as the start of a
                // per-frame record.
                info!(
                    "IVF: no DKIF file header on this stream (typical for gstreamer \
                     avmux_ivf on tcpserversink); parsing per-frame records directly"
                );
            }
            self.header_phase_done = true;
        }

        // Emit as many whole frames as we have.
        loop {
            if self.buf.len() < 12 {
                return;
            }
            let size =
                u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
            if size == 0 || size > MAX_FRAME_BYTES {
                warn!(
                    "IVF: implausible frame_size={size} bytes — byte stream is misaligned. \
                     Dropping connection so the main loop can reconnect and re-anchor on the \
                     next gstreamer buffer boundary."
                );
                self.desynced = true;
                self.buf.clear();
                return;
            }
            if self.buf.len() < 12 + size {
                return;
            }
            let frame = self.buf[12..12 + size].to_vec();
            self.buf.drain(..12 + size);
            out.push(frame);
        }
    }
}

/// Splits an incoming Annex-B bytestream into access units on AUD
/// boundaries. The AUD NAL type and NAL-type extraction are codec
/// specific — pass the right `CodecArg`.
///
/// Relies on the upstream parser emitting an AUD at the start of every
/// AU (`x264enc aud=true` for H.264, `x265enc option-string="aud=1"`
/// plumbed through `h265parse` for H.265). Bytes before the first AUD
/// are discarded; each subsequent AU is emitted when the *next* AU's
/// AUD arrives (so there's always one AU of buffering lag, bounded by
/// the frame interval).
struct AuSplitter {
    codec: CodecArg,
    buf: Vec<u8>,
    /// Offset (into `buf`) of the start code of the AU currently being
    /// accumulated. `None` before the first AUD has been observed.
    au_start: Option<usize>,
    /// Position up to which `buf` has already been scanned for start codes.
    scan_pos: usize,
}

impl AuSplitter {
    fn new(codec: CodecArg) -> Self {
        Self { codec, buf: Vec::with_capacity(256 * 1024), au_start: None, scan_pos: 0 }
    }

    fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.extend_from_slice(chunk);

        // Scan for start codes. We need 4 more bytes to decide (3-byte
        // start code + 1 NAL header byte). A 4-byte start code is detected
        // one byte earlier and handled naturally as "zero byte, then
        // 3-byte start code" collapsing into a 4-byte pattern.
        let aud = self.codec.aud_nal_type();
        while self.scan_pos + 3 < self.buf.len() {
            let i = self.scan_pos;
            let (sc_start, sc_len) = if i + 4 <= self.buf.len()
                && self.buf[i] == 0
                && self.buf[i + 1] == 0
                && self.buf[i + 2] == 0
                && self.buf[i + 3] == 1
            {
                // 4-byte start code at i. We still need the NAL header byte after it.
                if i + 5 > self.buf.len() {
                    break;
                }
                (i, 4)
            } else if self.buf[i] == 0 && self.buf[i + 1] == 0 && self.buf[i + 2] == 1 {
                (i, 3)
            } else {
                self.scan_pos += 1;
                continue;
            };

            let nal_off = sc_start + sc_len;
            if self.codec.nal_type(self.buf[nal_off]) == aud {
                // AUD — boundary between AUs.
                if let Some(start) = self.au_start.take() {
                    out.push(self.buf[start..sc_start].to_vec());
                }
                self.au_start = Some(sc_start);
            }
            self.scan_pos = nal_off + 1;
        }

        // Compact: drop bytes before the current AU start (or before the
        // last 3 bytes, in case a start code straddles the next feed).
        let drain_before = self.au_start.unwrap_or_else(|| self.buf.len().saturating_sub(3));
        if drain_before > 0 {
            self.buf.drain(..drain_before);
            self.scan_pos = self.scan_pos.saturating_sub(drain_before);
            if self.au_start.is_some() {
                self.au_start = Some(0);
            }
        }
    }
}

/// Minimal keyframe probe. For H.264/H.265 it scans for a keyframe
/// NAL (IDR slice / IRAP); for VP8 it reads bit 0 of the frame tag
/// (RFC 6386 §9.1: 0 = keyframe, 1 = interframe); for VP9 it decodes
/// the leading bits of the uncompressed header (VP9 bitstream spec
/// §6.2); for AV1 it scans the OBUs in the Temporal Unit for an
/// OBU_SEQUENCE_HEADER (which libaom/SVT-AV1/rav1e only emit at
/// keyframes — this is the same heuristic WebRTC's own AV1 RTP
/// packetizer uses).
fn is_keyframe(codec: CodecArg, data: &[u8]) -> bool {
    match codec {
        CodecArg::H264 | CodecArg::H265 => is_keyframe_annex_b(codec, data),
        CodecArg::Vp8 => !data.is_empty() && (data[0] & 0x01) == 0,
        CodecArg::Vp9 => is_keyframe_vp9(data),
        CodecArg::Av1 => is_keyframe_av1(data),
    }
}

/// AV1 keyframe probe. Walks the OBUs in a Temporal Unit and returns
/// true if any OBU has type `OBU_SEQUENCE_HEADER` (1). AV1 spec §5.3.2
/// (OBU header) + §5.3.1 (leb128):
///
/// * byte 0 bits 6..=3: `obu_type`.
/// * byte 0 bit 2: `obu_extension_flag`; if set, one extension byte
///   follows.
/// * byte 0 bit 1: `obu_has_size_field`; if set, a leb128-encoded
///   `obu_size` follows and gives the payload length. If clear, the
///   OBU runs to the end of the input (legacy AV1) — so we stop
///   scanning because we can't skip it.
///
/// Assumes the Low Overhead Bitstream Format produced by gstreamer's
/// `av1parse stream-format=obu-stream,alignment=tu` + `avmux_ivf`:
/// one Temporal Unit per IVF record, each OBU carries its own size.
fn is_keyframe_av1(mut data: &[u8]) -> bool {
    const OBU_SEQUENCE_HEADER: u8 = 1;
    while !data.is_empty() {
        let header = data[0];
        let obu_type = (header >> 3) & 0x0F;
        let ext = (header & 0x04) != 0;
        let has_size = (header & 0x02) != 0;

        let mut off = 1;
        if ext {
            if off >= data.len() {
                return false;
            }
            off += 1;
        }
        if !has_size {
            // No size field means we can't skip to the next OBU; treat
            // this OBU as the last one and decide based on what we've
            // seen so far.
            return obu_type == OBU_SEQUENCE_HEADER;
        }
        let (size, size_len) = match read_leb128(&data[off..]) {
            Some(v) => v,
            None => return false,
        };
        off += size_len;
        let payload_end = match off.checked_add(size as usize) {
            Some(e) if e <= data.len() => e,
            _ => return false,
        };
        if obu_type == OBU_SEQUENCE_HEADER {
            return true;
        }
        data = &data[payload_end..];
    }
    false
}

/// Decodes an AV1 leb128 (unsigned little-endian base-128) integer.
/// Returns `(value, bytes_consumed)` or `None` on truncated input.
/// AV1 spec §4.10.5 caps the encoding at 8 bytes and 32 significant
/// bits; we enforce the 8-byte limit and keep the value in a u32.
fn read_leb128(input: &[u8]) -> Option<(u32, usize)> {
    let mut value: u64 = 0;
    for (i, &byte) in input.iter().take(8).enumerate() {
        value |= ((byte & 0x7F) as u64) << (i * 7);
        if (byte & 0x80) == 0 {
            return u32::try_from(value).ok().map(|v| (v, i + 1));
        }
    }
    None
}

/// VP9 uncompressed-header keyframe probe. Reads first-byte bits (MSB
/// first) per VP9 bitstream spec §6.2:
///
/// * bits 7..=6: `frame_marker` (must be `0b10`).
/// * bit 5: `profile_low_bit`, bit 4: `profile_high_bit`
///   (combined `profile` ∈ 0..=3).
/// * For `profile == 3`: bit 3 is reserved-zero, bit 2 is
///   `show_existing_frame`, bit 1 is `frame_type`.
/// * For `profile != 3`: bit 3 is `show_existing_frame`, bit 2 is
///   `frame_type`.
///
/// A keyframe has `show_existing_frame == 0` and `frame_type == 0`.
/// `show_existing_frame == 1` records redisplay a previously decoded
/// buffer and carry no new coded data, so they are explicitly not
/// keyframes.
fn is_keyframe_vp9(data: &[u8]) -> bool {
    let Some(&b0) = data.first() else {
        return false;
    };
    if (b0 >> 6) & 0b11 != 0b10 {
        return false;
    }
    let profile_low = (b0 >> 5) & 0x1;
    let profile_high = (b0 >> 4) & 0x1;
    let profile = (profile_high << 1) | profile_low;
    let (show_existing_bit, frame_type_bit) = if profile == 3 { (2, 1) } else { (3, 2) };
    let show_existing = (b0 >> show_existing_bit) & 0x1;
    if show_existing != 0 {
        return false;
    }
    let frame_type = (b0 >> frame_type_bit) & 0x1;
    frame_type == 0
}

fn is_keyframe_annex_b(codec: CodecArg, data: &[u8]) -> bool {
    let mut i = 0usize;
    while i + 3 < data.len() {
        let is_four = i + 4 <= data.len()
            && data[i] == 0
            && data[i + 1] == 0
            && data[i + 2] == 0
            && data[i + 3] == 1;
        let is_three = data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1;
        if is_four || is_three {
            let payload_idx = if is_four { i + 4 } else { i + 3 };
            if payload_idx < data.len() && codec.is_keyframe_nal(codec.nal_type(data[payload_idx]))
            {
                return true;
            }
            i = payload_idx + 1;
        } else {
            i += 1;
        }
    }
    false
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let shutdown = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            shutdown.store(true, Ordering::Release);
            info!("Ctrl-C received, shutting down...");
        }
    });

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

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = false;
    room_options.dynacast = false;
    let (room, _events) = Room::connect(&args.url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} (sid {})", room.name(), room.sid().await);

    let resolution = VideoResolution { width: args.width, height: args.height };
    let source = NativeEncodedVideoSource::new(args.codec.webrtc_codec(), resolution);
    let target_bitrate_bps = Arc::new(AtomicU64::new(0));
    source.set_observer(Arc::new(LoggingObserver::new(target_bitrate_bps.clone())));
    info!(
        "Created encoded {} source: {}x{} (source_id={})",
        args.codec.name(),
        args.width,
        args.height,
        source.source_id()
    );

    let track_name = match args.codec {
        CodecArg::H264 => "encoded-h264",
        CodecArg::H265 => "encoded-h265",
        CodecArg::Vp8 => "encoded-vp8",
        CodecArg::Vp9 => "encoded-vp9",
        CodecArg::Av1 => "encoded-av1",
    };
    let track =
        LocalVideoTrack::create_video_track(track_name, RtcVideoSource::Encoded(source.clone()));

    let publish_opts = TrackPublishOptions {
        source: TrackSource::Camera,
        simulcast: false,
        video_codec: args.codec.livekit_codec(),
        video_encoding: Some(VideoEncoding {
            max_bitrate: args.max_bitrate_kbps.saturating_mul(1000),
            max_framerate: args.max_framerate,
        }),
        ..Default::default()
    };
    room.local_participant()
        .publish_track(LocalTrack::Video(track), publish_opts)
        .await
        .context("publish_track failed")?;
    info!(
        "Published encoded {} track (max {} kbps @ {:.1} fps)",
        args.codec.name(),
        args.max_bitrate_kbps,
        args.max_framerate
    );

    let frames_accepted = Arc::new(AtomicU64::new(0));
    let frames_dropped = Arc::new(AtomicU64::new(0));
    let keyframes = Arc::new(AtomicU64::new(0));
    let encoded_bytes = Arc::new(AtomicU64::new(0));

    {
        let frames_accepted = frames_accepted.clone();
        let frames_dropped = frames_dropped.clone();
        let keyframes = keyframes.clone();
        let encoded_bytes = encoded_bytes.clone();
        let target_bitrate_bps = target_bitrate_bps.clone();
        tokio::spawn(async move {
            let mut last = Instant::now();
            loop {
                sleep(Duration::from_secs(2)).await;
                let elapsed = last.elapsed().as_secs_f64();
                last = Instant::now();
                let ok = frames_accepted.swap(0, Ordering::Relaxed);
                let dropped = frames_dropped.swap(0, Ordering::Relaxed);
                let kf = keyframes.swap(0, Ordering::Relaxed);
                let bytes = encoded_bytes.swap(0, Ordering::Relaxed);
                if ok + dropped > 0 {
                    let encoded_kbps = bytes as f64 * 8.0 / elapsed / 1000.0;
                    let target_kbps = target_bitrate_bps.load(Ordering::Relaxed) / 1000;
                    info!(
                        "ingest: {:.1} fps accepted, {:.1} fps dropped, {:.0} kbps encoded \
                         (target {} kbps), {} keyframes",
                        ok as f64 / elapsed,
                        dropped as f64 / elapsed,
                        encoded_kbps,
                        target_kbps,
                        kf
                    );
                }
            }
        });
    }

    // Reconnect loop: if gstreamer restarts, we come back up automatically.
    while !shutdown.load(Ordering::Acquire) {
        let addr = format!("{}:{}", args.tcp_host, args.tcp_port);
        let framing = match args.codec {
            CodecArg::H264 | CodecArg::H265 => "Annex-B",
            CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => "IVF",
        };
        info!("Connecting to {addr} for {} {framing} bytestream...", args.codec.name());
        let mut stream = match TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                warn!("connect {addr} failed: {e}. Retrying in 1s...");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        info!("Connected to {addr}");

        let mut demuxer = Demuxer::new(args.codec);
        let mut read_buf = vec![0u8; 64 * 1024];
        let mut out = Vec::new();
        loop {
            if shutdown.load(Ordering::Acquire) {
                break;
            }
            let n = tokio::select! {
                r = stream.read(&mut read_buf) => r,
                _ = sleep(Duration::from_millis(250)) => continue,
            };
            let n = match n {
                Ok(0) => {
                    warn!("gstreamer closed the connection");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {e}");
                    break;
                }
            };

            out.clear();
            demuxer.feed(&read_buf[..n], &mut out);
            if demuxer.desynced() {
                warn!("demuxer reported desync — dropping TCP connection to re-align");
                break;
            }
            for au in out.drain(..) {
                encoded_bytes.fetch_add(au.len() as u64, Ordering::Relaxed);
                let is_keyframe = is_keyframe(args.codec, &au);
                if is_keyframe {
                    keyframes.fetch_add(1, Ordering::Relaxed);
                }
                let info = EncodedFrameInfo {
                    is_keyframe,
                    has_sps_pps: false, // the source scans+prepends SPS/PPS as needed
                    width: args.width,
                    height: args.height,
                    capture_time_us: 0,
                };
                if source.capture_frame(&au, &info) {
                    frames_accepted.fetch_add(1, Ordering::Relaxed);
                } else {
                    frames_dropped.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        "capture_frame dropped AU ({} bytes, keyframe={})",
                        au.len(),
                        is_keyframe
                    );
                }
            }
        }

        if !shutdown.load(Ordering::Acquire) {
            sleep(Duration::from_secs(1)).await;
        }
    }

    info!("Shutting down...");
    Ok(())
}
