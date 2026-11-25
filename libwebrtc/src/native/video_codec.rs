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

//! Standalone video encoding and decoding, outside of a PeerConnection.
//!
//! [`VideoEncoder`] and [`VideoDecoder`] are created from the same codec
//! factories the PeerConnection uses, so they offer the same codecs and
//! hardware backends.
//!
//! Bitstreams use the canonical format of each codec, on both sides:
//!
//! - H.264 / H.265: Annex B, with parameter sets in-band on keyframes. H.264
//!   keyframes are processed like a WebRTC sender's (see
//!   [`rewrite_h264_keyframe`])
//! - VP8: one frame
//! - VP9: one frame
//! - AV1: the low-overhead OBUs of one temporal unit, starting with a
//!   temporal delimiter

use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use cxx::UniquePtr;
use parking_lot::Mutex;
use thiserror::Error;
use tokio::sync::mpsc;
use webrtc_sys::{video_codec as vc_sys, video_frame as vf_sys, video_track as vt_sys};

use super::video_frame::new_video_frame_buffer;
use crate::{
    rtp_sender::VideoEncoderBackend,
    video_frame::{
        BoxVideoFrame, EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame, FrameMetadata,
        VideoBuffer, VideoFrame,
    },
    video_source::VideoResolution,
};

/// Encoder output buffered until [`VideoEncoder::recv`] picks it up.
const ENCODED_QUEUE_CAPACITY: usize = 64;
/// Decoder output buffered until [`VideoDecoder::recv`] picks it up.
const DECODED_QUEUE_CAPACITY: usize = 8;
/// Input frames whose timestamp and metadata are remembered until the
/// matching output arrives.
const PENDING_FRAMES_CAPACITY: usize = 64;
/// RTP timestamp spacing used for decoder input (90 kHz at 30 fps); decoders
/// only need it to be unique and increasing.
const DECODER_RTP_STEP: u32 = 3000;

/// One simulcast layer of a [`VideoEncoderConfig`].
#[derive(Debug, Clone, PartialEq)]
pub struct SimulcastLayer {
    /// Width of the layer in pixels.
    pub width: u32,
    /// Height of the layer in pixels.
    pub height: u32,
    /// Maximum bitrate of the layer in bits per second.
    pub max_bitrate_bps: u32,
    /// Maximum framerate of the layer, or `0.0` for
    /// [`VideoEncoderConfig::max_framerate`].
    pub max_framerate: f64,
    /// Whether the layer is encoded from the start, see
    /// [`VideoEncoder::set_layer_active`].
    pub active: bool,
}

/// Configuration of a [`VideoEncoder`].
#[derive(Debug, Clone, PartialEq)]
pub struct VideoEncoderConfig {
    /// Codec to encode with.
    pub codec: EncodedVideoCodec,
    /// Preferred encoder backend. A backend that cannot encode `codec` falls
    /// back to another one, as on an [`RtpSender`](crate::rtp_sender::RtpSender);
    /// see [`VideoEncoder::info`] for the one in use.
    pub backend: VideoEncoderBackend,
    /// Input width in pixels. Frames of another size are scaled to it.
    pub width: u32,
    /// Input height in pixels. Frames of another size are scaled to it.
    pub height: u32,
    /// Maximum input framerate.
    pub max_framerate: f64,
    /// Initial total bitrate, allocated to the active layers from the lowest
    /// one up, each capped at its maximum. `0` means the sum of the maxima.
    pub start_bitrate_bps: u32,
    /// Minimum bitrate of the lowest layer.
    pub min_bitrate_bps: u32,
    /// Maximum bitrate when [`layers`](Self::layers) is empty.
    pub max_bitrate_bps: u32,
    /// Simulcast layers, ordered from lowest to highest quality. The last one
    /// must be `width`x`height`. Empty for a single layer.
    pub layers: Vec<SimulcastLayer>,
    /// Temporal layers of every simulcast layer, `1` (no temporal
    /// scalability) to `3` (`L1T3`). VP8, VP9 and AV1 only; see
    /// [`EncodedVideoOutput::temporal_id`].
    pub temporal_layers: u32,
}

impl VideoEncoderConfig {
    /// Creates a single-layer configuration at 30 fps using the default
    /// backend.
    pub fn new(codec: EncodedVideoCodec, width: u32, height: u32, max_bitrate_bps: u32) -> Self {
        Self {
            codec,
            backend: VideoEncoderBackend::Auto,
            width,
            height,
            max_framerate: 30.0,
            start_bitrate_bps: 0,
            min_bitrate_bps: 0,
            max_bitrate_bps,
            layers: Vec::new(),
            temporal_layers: 1,
        }
    }
}

/// The implementation behind a [`VideoEncoder`] or [`VideoDecoder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoCodecInfo {
    /// Name of the implementation, e.g. `libvpx` or `VideoToolbox`.
    pub implementation_name: String,
    /// Whether the implementation is hardware accelerated.
    pub is_hardware_accelerated: bool,
}

impl From<vc_sys::ffi::VideoCodecInfo> for VideoCodecInfo {
    fn from(info: vc_sys::ffi::VideoCodecInfo) -> Self {
        Self {
            implementation_name: info.implementation_name,
            is_hardware_accelerated: info.is_hardware_accelerated,
        }
    }
}

/// One encoded frame of one layer, produced by a [`VideoEncoder`].
#[derive(Debug, Clone)]
pub struct EncodedVideoOutput {
    /// Codec of the bitstream.
    pub codec: EncodedVideoCodec,
    /// Index of the layer in [`VideoEncoderConfig::layers`] (`0` without
    /// simulcast).
    pub layer: usize,
    /// Whether this is a keyframe.
    pub frame_type: EncodedFrameType,
    /// Encoded resolution.
    pub resolution: VideoResolution,
    /// [`VideoFrame::timestamp_us`] of the input frame.
    pub timestamp_us: i64,
    /// [`VideoFrame::frame_metadata`] of the input frame.
    pub frame_metadata: Option<FrameMetadata>,
    /// The bitstream, see the [module documentation](self).
    pub data: Vec<u8>,
    /// Quantizer the frame was encoded with, when the encoder reports it.
    pub qp: Option<u8>,
    /// Time from the start of the input frame's encode to this output. For
    /// simulcast, later layers include the time spent on earlier ones.
    pub encode_time: Duration,
    /// Temporal layer of the frame when encoding with
    /// [`VideoEncoderConfig::temporal_layers`] above 1: frames of a layer
    /// only reference frames of the same or lower layers, so higher layers
    /// can be dropped.
    pub temporal_id: Option<u8>,
}

impl EncodedVideoOutput {
    /// Whether this is a keyframe.
    pub fn is_keyframe(&self) -> bool {
        self.frame_type == EncodedFrameType::Key
    }

    /// Borrows this output as decoder or pre-encoded source input.
    pub fn as_encoded_video_frame(&self) -> EncodedVideoFrame<'_> {
        EncodedVideoFrame {
            codec: self.codec,
            payload: &self.data,
            timestamp_us: self.timestamp_us,
            frame_type: self.frame_type,
            resolution: self.resolution.clone(),
            frame_metadata: self.frame_metadata.clone(),
        }
    }
}

/// Error creating a [`VideoEncoder`] or [`VideoDecoder`].
#[derive(Debug, Error)]
#[error("failed to create video codec: {reason}")]
pub struct VideoCodecCreateError {
    /// Why the codec could not be created.
    pub reason: String,
}

/// Error returned by [`VideoEncoder::encode`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncodeError {
    /// The encoder is still busy with earlier frames; this frame was dropped.
    #[error("video encoder is busy, frame dropped")]
    Busy,
    /// The frame carries a pre-encoded buffer, which cannot be re-encoded.
    #[error("frame cannot be encoded")]
    InvalidFrame,
    /// An earlier frame failed to encode with this `WEBRTC_VIDEO_CODEC_*`
    /// error code; this frame was dropped. Hardware encoders fail this way
    /// when their session breaks; recreating the encoder (possibly with
    /// [`VideoEncoderBackend::Software`]) recovers.
    #[error("video encoder failed with error {0}")]
    EncoderFailed(i32),
}

/// Error returned by [`VideoDecoder::decode`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum VideoDecodeError {
    /// The frame is not of the decoder's codec.
    #[error("frame codec does not match the decoder")]
    CodecMismatch,
    /// The frame has no payload.
    #[error("empty frame")]
    EmptyFrame,
    /// The frame was not decoded because the decoder lost sync: no keyframe
    /// was decoded yet, an earlier frame failed to decode, or the decoder
    /// fell too far behind and dropped a frame. Delta frames are rejected
    /// until a keyframe is supplied, so the caller should request one.
    #[error("keyframe required")]
    KeyframeRequired,
}

/// Cumulative statistics of a [`VideoDecoder`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VideoDecoderStats {
    /// Frames the decoder produced.
    pub frames_decoded: u64,
    /// Keyframes accepted by [`VideoDecoder::decode`].
    pub key_frames_submitted: u64,
    /// Decoded frames discarded because [`VideoDecoder::recv`] fell behind.
    pub frames_dropped: u64,
    /// Frames the native decoder failed to decode.
    pub decode_errors: u64,
    /// Decoder time spent on the decoded frames.
    pub total_decode_time: Duration,
    /// Sum of the QPs of the decoded frames that reported one.
    pub qp_sum: u64,
}

/// Applies WebRTC's send-side processing of H.264 keyframes to an Annex B
/// access unit: SPSs without bitstream restrictions get them (bounding decoder
/// latency), and access unit delimiters are dropped. [`VideoEncoder`] does it
/// to its own output; apply it to H.264 keyframes encoded elsewhere before
/// encrypting them, since WebRTC receivers apply the same rewrite on receipt,
/// and frame encryption authenticates the parameter sets.
pub fn rewrite_h264_keyframe(data: &[u8]) -> Vec<u8> {
    vc_sys::ffi::rewrite_h264_keyframe(data)
}

/// Remembers the timestamp and metadata of input frames by RTP timestamp, so
/// the codec output can carry them.
#[derive(Default)]
struct PendingFrames {
    entries: VecDeque<(u32, i64, Option<FrameMetadata>)>,
}

impl PendingFrames {
    fn insert(&mut self, rtp_timestamp: u32, timestamp_us: i64, meta: Option<FrameMetadata>) {
        if self.entries.len() >= PENDING_FRAMES_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((rtp_timestamp, timestamp_us, meta));
    }

    fn get(&self, rtp_timestamp: u32) -> Option<(i64, Option<FrameMetadata>)> {
        self.entries
            .iter()
            .rev()
            .find(|(rtp, _, _)| *rtp == rtp_timestamp)
            .map(|(_, timestamp_us, meta)| (*timestamp_us, meta.clone()))
    }
}

fn now_us() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |now| now.as_micros() as i64)
}

fn sys_codec(codec: EncodedVideoCodec) -> vt_sys::ffi::EncodedVideoCodec {
    codec.into()
}

/// AV1 output must be a full temporal unit, which starts with a temporal
/// delimiter OBU. Encoders may leave it out since RTP does not carry it.
fn with_temporal_delimiter(data: &[u8]) -> Vec<u8> {
    const OBU_TEMPORAL_DELIMITER: u8 = 2;
    let starts_with_delimiter =
        data.first().is_some_and(|header| (header >> 3) & 0x0f == OBU_TEMPORAL_DELIMITER);
    if starts_with_delimiter {
        return data.to_vec();
    }
    // obu_type = 2, obu_has_size_field = 1, obu_size = 0.
    let mut unit = Vec::with_capacity(data.len() + 2);
    unit.extend_from_slice(&[0x12, 0x00]);
    unit.extend_from_slice(data);
    unit
}

/// Receives encoder output on encoder threads and queues it for
/// [`VideoEncoder::recv`].
struct EncoderSink {
    codec: EncodedVideoCodec,
    /// Whether frames carry temporal layer ids (some encoders report layer 0 without).
    temporal_layers: bool,
    pending: Arc<Mutex<PendingFrames>>,
    sender: mpsc::Sender<EncodedVideoOutput>,
}

impl vc_sys::EncodedVideoSink for EncoderSink {
    fn on_encoded_frame(&self, info: &vc_sys::ffi::EncodedVideoFrameInfo, data: &[u8]) -> bool {
        let (timestamp_us, frame_metadata) = self
            .pending
            .lock()
            .get(info.rtp_timestamp)
            .unwrap_or((i64::from(info.rtp_timestamp) * 1000 / 90, None));
        let data = match self.codec {
            EncodedVideoCodec::AV1 => with_temporal_delimiter(data),
            _ => data.to_vec(),
        };
        let output = EncodedVideoOutput {
            codec: self.codec,
            layer: info.layer as usize,
            frame_type: if info.is_keyframe {
                EncodedFrameType::Key
            } else {
                EncodedFrameType::Delta
            },
            resolution: VideoResolution { width: info.width, height: info.height },
            timestamp_us,
            frame_metadata,
            data,
            qp: u8::try_from(info.qp).ok(),
            encode_time: Duration::from_micros(u64::from(info.encode_time_us)),
            temporal_id: u8::try_from(info.temporal_id).ok().filter(|_| self.temporal_layers),
        };
        // On failure the native encoder restarts the layer with a keyframe.
        self.sender.try_send(output).is_ok()
    }
}

/// Encodes raw [`VideoFrame`]s, optionally into several simulcast layers.
///
/// Every input frame produces one [`EncodedVideoOutput`] per active layer,
/// retrieved with [`recv`](Self::recv) or [`try_recv`](Self::try_recv). The
/// first output of each layer is a keyframe; later keyframes are produced on
/// [`request_keyframe`](Self::request_keyframe) and, for some encoders,
/// periodically.
///
/// The encoder runs on its own task queue, so [`encode`](Self::encode) only
/// queues the frame and returns immediately; hardware encoders may also
/// produce output on their own threads. Up to 64 encoded frames are buffered;
/// when the buffer is full, further output is dropped and the affected layer
/// restarts with a keyframe. Dropping the encoder releases the native encoder
/// and its queue before returning.
///
/// ```no_run
/// use libwebrtc::{
///     native::video_codec::{VideoEncoder, VideoEncoderConfig},
///     video_frame::{EncodedVideoCodec, I420Buffer, VideoFrame, VideoRotation},
/// };
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let config = VideoEncoderConfig::new(EncodedVideoCodec::VP8, 640, 360, 800_000);
/// let mut encoder = VideoEncoder::new(&config)?;
/// encoder.encode(&VideoFrame::new(VideoRotation::VideoRotation0, I420Buffer::new(640, 360)))?;
/// let output = encoder.recv().await.unwrap();
/// assert!(output.is_keyframe());
/// # Ok(())
/// # }
/// ```
pub struct VideoEncoder {
    // Dropped first: tears the native encoder down, which stops all output.
    handle: UniquePtr<vc_sys::ffi::NativeVideoEncoder>,
    receiver: mpsc::Receiver<EncodedVideoOutput>,
    pending: Arc<Mutex<PendingFrames>>,
    last_rtp_timestamp: Mutex<Option<u32>>,
    num_layers: usize,
}

impl VideoEncoder {
    /// Creates and initializes an encoder.
    ///
    /// Blocks while the native encoder initializes, which takes a few
    /// milliseconds (longer for some hardware encoders).
    pub fn new(config: &VideoEncoderConfig) -> Result<Self, VideoCodecCreateError> {
        let settings = vc_sys::ffi::VideoEncoderSettings {
            codec: sys_codec(config.codec),
            backend: config.backend.into(),
            width: config.width,
            height: config.height,
            max_framerate: config.max_framerate,
            start_bitrate_bps: config.start_bitrate_bps,
            min_bitrate_bps: config.min_bitrate_bps,
            max_bitrate_bps: config.max_bitrate_bps,
            layers: config
                .layers
                .iter()
                .map(|layer| vc_sys::ffi::VideoEncoderLayerSettings {
                    width: layer.width,
                    height: layer.height,
                    max_bitrate_bps: layer.max_bitrate_bps,
                    max_framerate: layer.max_framerate,
                    active: layer.active,
                })
                .collect(),
            num_temporal_layers: config.temporal_layers.max(1),
        };

        let pending = Arc::new(Mutex::new(PendingFrames::default()));
        let (sender, receiver) = mpsc::channel(ENCODED_QUEUE_CAPACITY);
        let sink = EncoderSink {
            codec: config.codec,
            temporal_layers: config.temporal_layers > 1,
            pending: pending.clone(),
            sender,
        };
        let observer = Box::new(vc_sys::VideoEncoderObserver::new(Box::new(sink)));
        let handle = vc_sys::ffi::create_video_encoder(&settings, observer)
            .map_err(|e| VideoCodecCreateError { reason: e.what().to_owned() })?;

        Ok(Self {
            handle,
            receiver,
            pending,
            last_rtp_timestamp: Mutex::new(None),
            num_layers: config.layers.len().max(1),
        })
    }

    /// Queues `frame` for encoding.
    ///
    /// The frame's rotation is applied to the pixels, and a frame whose size
    /// differs from the configured one is scaled to it. Its
    /// [`timestamp_us`](VideoFrame::timestamp_us) (the current time when `0`)
    /// and [`frame_metadata`](VideoFrame::frame_metadata) are passed through
    /// to the outputs.
    pub fn encode<T: AsRef<dyn VideoBuffer>>(
        &self,
        frame: &VideoFrame<T>,
    ) -> Result<(), VideoEncodeError> {
        let timestamp_us = if frame.timestamp_us == 0 { now_us() } else { frame.timestamp_us };
        let rtp_timestamp = self.next_rtp_timestamp(timestamp_us);

        let mut builder = vf_sys::ffi::new_video_frame_builder();
        builder.pin_mut().set_rotation(frame.rotation.into());
        builder.pin_mut().set_video_frame_buffer(frame.buffer.as_ref().sys_handle());
        builder.pin_mut().set_timestamp_us(timestamp_us);
        let native_frame = builder.pin_mut().build();

        self.pending.lock().insert(rtp_timestamp, timestamp_us, frame.frame_metadata.clone());

        let result = self.handle.encode(&native_frame, rtp_timestamp);
        match result.status {
            vc_sys::ffi::VideoEncodeStatus::Queued => Ok(()),
            vc_sys::ffi::VideoEncodeStatus::Busy => Err(VideoEncodeError::Busy),
            vc_sys::ffi::VideoEncodeStatus::InvalidFrame => Err(VideoEncodeError::InvalidFrame),
            _ => Err(VideoEncodeError::EncoderFailed(result.error_code)),
        }
    }

    /// Waits for the next encoded frame.
    ///
    /// Only returns `None` once the encoder is gone, so it never does while
    /// `self` is borrowed: callers waiting for output must also stop waiting
    /// when they stop feeding frames.
    pub async fn recv(&mut self) -> Option<EncodedVideoOutput> {
        self.receiver.recv().await
    }

    /// Returns the next encoded frame, if one is ready.
    pub fn try_recv(&mut self) -> Option<EncodedVideoOutput> {
        self.receiver.try_recv().ok()
    }

    /// Makes the next encoded frame of `layer` (of every layer when `None`) a
    /// keyframe.
    ///
    /// VP9 simulcast is encoded as one stream whose layers only restart
    /// together, so there every layer gets a keyframe.
    pub fn request_keyframe(&self, layer: Option<usize>) {
        let layer = match layer {
            Some(layer) if layer < self.num_layers => layer as i32,
            Some(_) => return,
            None => -1,
        };
        self.handle.request_keyframe(layer);
    }

    /// Sets the target bitrate of each layer, in layer order, and the
    /// expected input framerate.
    ///
    /// A layer with a target of `0` is paused, like an inactive one. Missing
    /// trailing entries keep their current target.
    pub fn set_rates(&self, layer_bitrates_bps: &[u32], framerate_fps: f64) {
        self.handle.set_rates(layer_bitrates_bps, framerate_fps);
    }

    /// Starts or stops encoding `layer`, e.g. for dynacast.
    ///
    /// A resumed layer starts with a keyframe.
    pub fn set_layer_active(&self, layer: usize, active: bool) {
        if let Ok(layer) = u32::try_from(layer) {
            self.handle.set_layer_active(layer, active);
        }
    }

    /// Describes the encoder implementation in use.
    pub fn info(&self) -> VideoCodecInfo {
        self.handle.info().into()
    }

    /// State shared with the native encoder's callback; gone once the native
    /// encoder released the callback.
    #[cfg(test)]
    pub(crate) fn callback_state(&self) -> std::sync::Weak<impl Send + Sync> {
        Arc::downgrade(&self.pending)
    }

    /// Converts `timestamp_us` to a 90 kHz RTP timestamp, strictly increasing
    /// across calls as encoders require.
    fn next_rtp_timestamp(&self, timestamp_us: i64) -> u32 {
        let rtp_timestamp = (timestamp_us.wrapping_mul(90) / 1000) as u32;
        let mut last = self.last_rtp_timestamp.lock();
        let rtp_timestamp = match *last {
            Some(last) if (rtp_timestamp.wrapping_sub(last) as i32) <= 0 => last.wrapping_add(1),
            _ => rtp_timestamp,
        };
        *last = Some(rtp_timestamp);
        rtp_timestamp
    }
}

/// Receives decoder output on decoder threads and queues it for
/// [`VideoDecoder::recv`].
struct DecoderSink {
    pending: Arc<Mutex<PendingFrames>>,
    sender: mpsc::Sender<BoxVideoFrame>,
    stats: Arc<Mutex<VideoDecoderStats>>,
}

impl vc_sys::DecodedVideoSink for DecoderSink {
    fn on_decoded_frame(
        &self,
        frame: UniquePtr<vf_sys::ffi::VideoFrame>,
        info: &vc_sys::ffi::DecodedVideoFrameInfo,
    ) {
        let (timestamp_us, frame_metadata) =
            self.pending.lock().get(frame.timestamp()).unwrap_or((frame.timestamp_us(), None));
        let output: BoxVideoFrame = VideoFrame {
            rotation: frame.rotation().into(),
            timestamp_us,
            frame_metadata,
            // SAFETY: `frame` is a valid frame handed over by the decoder.
            buffer: new_video_frame_buffer(unsafe { frame.video_frame_buffer() }),
        };
        let delivered = self.sender.try_send(output).is_ok();
        let mut stats = self.stats.lock();
        stats.frames_decoded += 1;
        stats.total_decode_time += Duration::from_micros(u64::from(info.decode_time_us));
        stats.qp_sum += u64::try_from(info.qp).unwrap_or_default();
        if !delivered {
            stats.frames_dropped += 1;
            log::debug!("video decoder output queue full, dropping a decoded frame");
        }
    }
}

/// Decodes encoded frames into raw [`VideoFrame`]s.
///
/// Decoded frames are retrieved with [`recv`](Self::recv) or
/// [`try_recv`](Self::try_recv), carrying the
/// [`timestamp_us`](EncodedVideoFrame::timestamp_us) and
/// [`frame_metadata`](EncodedVideoFrame::frame_metadata) of their input. Their
/// buffer is I420 for software decoders and a native buffer (convertible with
/// `to_i420`) for hardware ones.
///
/// The decoder runs on its own task queue, so [`decode`](Self::decode) only
/// queues the frame and returns immediately. Up to 8 decoded frames are
/// buffered; when the buffer is full, newly decoded frames are dropped.
/// Dropping the decoder releases the native decoder and its queue before
/// returning.
pub struct VideoDecoder {
    // Dropped first: tears the native decoder down, which stops all output.
    handle: UniquePtr<vc_sys::ffi::NativeVideoDecoder>,
    receiver: mpsc::Receiver<BoxVideoFrame>,
    pending: Arc<Mutex<PendingFrames>>,
    stats: Arc<Mutex<VideoDecoderStats>>,
    next_rtp_timestamp: Mutex<u32>,
    codec: EncodedVideoCodec,
}

impl VideoDecoder {
    /// Creates a decoder for `codec`, preferring hardware decoders like a
    /// PeerConnection does.
    ///
    /// Blocks while the native decoder initializes.
    pub fn new(codec: EncodedVideoCodec) -> Result<Self, VideoCodecCreateError> {
        Self::create(codec, false)
    }

    /// Creates one of libwebrtc's software decoders for `codec`, e.g. to
    /// recover from a failing hardware decoder. Not every codec has one
    /// (H.265 does not, nor does H.264 when libwebrtc was built without it).
    pub fn new_software(codec: EncodedVideoCodec) -> Result<Self, VideoCodecCreateError> {
        Self::create(codec, true)
    }

    fn create(
        codec: EncodedVideoCodec,
        prefer_software: bool,
    ) -> Result<Self, VideoCodecCreateError> {
        let pending = Arc::new(Mutex::new(PendingFrames::default()));
        let stats = Arc::new(Mutex::new(VideoDecoderStats::default()));
        let (sender, receiver) = mpsc::channel(DECODED_QUEUE_CAPACITY);
        let sink = DecoderSink { pending: pending.clone(), sender, stats: stats.clone() };
        let observer = Box::new(vc_sys::VideoDecoderObserver::new(Box::new(sink)));
        let handle = vc_sys::ffi::create_video_decoder(sys_codec(codec), prefer_software, observer)
            .map_err(|e| VideoCodecCreateError { reason: e.what().to_owned() })?;
        Ok(Self { handle, receiver, pending, stats, next_rtp_timestamp: Mutex::new(0), codec })
    }

    /// Queues `frame` for decoding.
    ///
    /// Frames must be supplied in decoding order, starting with a keyframe.
    /// On [`VideoDecodeError::KeyframeRequired`], request a keyframe from the
    /// sender and keep feeding frames: delta frames are rejected until one
    /// arrives.
    pub fn decode(&self, frame: &EncodedVideoFrame<'_>) -> Result<(), VideoDecodeError> {
        if frame.codec != self.codec {
            return Err(VideoDecodeError::CodecMismatch);
        }
        if frame.payload.is_empty() {
            return Err(VideoDecodeError::EmptyFrame);
        }

        let rtp_timestamp = {
            let mut next = self.next_rtp_timestamp.lock();
            let rtp_timestamp = *next;
            *next = next.wrapping_add(DECODER_RTP_STEP);
            rtp_timestamp
        };
        let timestamp_us = if frame.timestamp_us == 0 { now_us() } else { frame.timestamp_us };
        self.pending.lock().insert(rtp_timestamp, timestamp_us, frame.frame_metadata.clone());

        let status = self.handle.decode(
            frame.payload,
            frame.frame_type == EncodedFrameType::Key,
            rtp_timestamp,
            frame.resolution.width,
            frame.resolution.height,
        );
        match status {
            vc_sys::ffi::VideoDecodeStatus::Queued => {
                if frame.frame_type == EncodedFrameType::Key {
                    self.stats.lock().key_frames_submitted += 1;
                }
                Ok(())
            }
            vc_sys::ffi::VideoDecodeStatus::InvalidInput => Err(VideoDecodeError::EmptyFrame),
            _ => Err(VideoDecodeError::KeyframeRequired),
        }
    }

    /// Cumulative statistics since the decoder was created.
    pub fn stats(&self) -> VideoDecoderStats {
        VideoDecoderStats { decode_errors: self.handle.decode_error_count(), ..*self.stats.lock() }
    }

    /// Waits for the next decoded frame.
    ///
    /// Only returns `None` once the decoder is gone, so it never does while
    /// `self` is borrowed.
    pub async fn recv(&mut self) -> Option<BoxVideoFrame> {
        self.receiver.recv().await
    }

    /// Returns the next decoded frame, if one is ready.
    pub fn try_recv(&mut self) -> Option<BoxVideoFrame> {
        self.receiver.try_recv().ok()
    }

    /// Describes the decoder implementation in use.
    pub fn info(&self) -> VideoCodecInfo {
        self.handle.info().into()
    }

    /// State shared with the native decoder's callback; gone once the native
    /// decoder released the callback.
    #[cfg(test)]
    pub(crate) fn callback_state(&self) -> std::sync::Weak<impl Send + Sync> {
        Arc::downgrade(&self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::{with_temporal_delimiter, PendingFrames, PENDING_FRAMES_CAPACITY};

    #[tokio::test]
    async fn standalone_codec_lifecycle() {
        use super::*;
        use crate::audio_frame::AudioFrame;
        use crate::native::audio_codec::{OpusDecoder, OpusEncoder, OpusEncoderConfig};
        use crate::video_frame::{I420Buffer, VideoRotation};
        use webrtc_sys::video_codec::ffi::live_video_codec_count;

        let baseline = live_video_codec_count();
        for _ in 0..6 {
            let cfg = VideoEncoderConfig::new(EncodedVideoCodec::VP8, 64, 48, 100_000);
            let mut enc = VideoEncoder::new(&cfg).unwrap();
            let mut dec = VideoDecoder::new(EncodedVideoCodec::VP8).unwrap();
            let callbacks = (enc.callback_state(), dec.callback_state());
            let frame = VideoFrame::new(VideoRotation::VideoRotation0, I420Buffer::new(64, 48));
            enc.encode(&frame).unwrap();
            let packet =
                tokio::time::timeout(Duration::from_secs(3), enc.recv()).await.unwrap().unwrap();
            dec.decode(&packet.as_encoded_video_frame()).unwrap();
            drop((enc, dec));
            assert!(callbacks.0.upgrade().is_none());
            assert!(callbacks.1.upgrade().is_none());
            let mut invalid = cfg;
            invalid.width = 0;
            assert!(VideoEncoder::new(&invalid).is_err());
            assert_eq!(live_video_codec_count(), baseline);

            let mut opus = OpusEncoder::new(&OpusEncoderConfig::default()).unwrap();
            let mut decoder = OpusDecoder::new(48000, 1).unwrap();
            let pcm = AudioFrame::new(48000, 1, 480);
            opus.encode(&pcm).unwrap();
            let packet = opus.encode(&pcm).unwrap().unwrap();
            assert_eq!(decoder.decode(&packet.data).unwrap().samples_per_channel, 960);
            assert!(OpusDecoder::new(48000, 0).is_err());
            assert!(OpusEncoder::new(&OpusEncoderConfig { num_channels: 0, ..Default::default() })
                .is_err());
        }
    }

    #[test]
    fn temporal_delimiter_is_added_once() {
        let sequence_header = [0x0a, 0x01, 0x00];
        assert_eq!(with_temporal_delimiter(&sequence_header), [0x12, 0x00, 0x0a, 0x01, 0x00]);
        assert_eq!(with_temporal_delimiter(&[0x12, 0x00, 0x32]), [0x12, 0x00, 0x32]);
    }

    #[test]
    fn pending_frames_are_bounded() {
        let mut pending = PendingFrames::default();
        for rtp in 0..(PENDING_FRAMES_CAPACITY as u32 + 10) {
            pending.insert(rtp, i64::from(rtp), None);
        }
        assert_eq!(pending.entries.len(), PENDING_FRAMES_CAPACITY);
        assert!(pending.get(0).is_none());
        assert_eq!(pending.get(PENDING_FRAMES_CAPACITY as u32 + 9).map(|(ts, _)| ts), Some(73));
    }
}
