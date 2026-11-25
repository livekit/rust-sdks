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

//! Standalone video encoders and decoders, created from the same factories
//! the PeerConnectionFactory uses.

use cxx::UniquePtr;

use crate::{impl_thread_safety, video_frame::ffi::VideoFrame};

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    /// One simulcast stream. Streams are ordered from lowest to highest
    /// quality, and the last one must match the encoder input size.
    #[derive(Debug, Clone)]
    pub struct VideoEncoderLayerSettings {
        pub width: u32,
        pub height: u32,
        pub max_bitrate_bps: u32,
        pub max_framerate: f64,
        pub active: bool,
    }

    #[derive(Debug, Clone)]
    pub struct VideoEncoderSettings {
        pub codec: EncodedVideoCodec,
        pub backend: VideoEncoderBackend,
        pub width: u32,
        pub height: u32,
        pub max_framerate: f64,
        pub start_bitrate_bps: u32,
        pub min_bitrate_bps: u32,
        pub max_bitrate_bps: u32,
        /// Empty for a single stream of `width`x`height`.
        pub layers: Vec<VideoEncoderLayerSettings>,
        /// Temporal layers of every stream (1 to 3; VP8, VP9 and AV1 only).
        pub num_temporal_layers: u32,
    }

    /// Describes one encoded frame handed to [`VideoEncoderObserver`].
    #[derive(Debug, Clone)]
    pub struct EncodedVideoFrameInfo {
        /// Simulcast stream index.
        pub layer: u32,
        pub is_keyframe: bool,
        pub width: u32,
        pub height: u32,
        /// RTP timestamp of the input frame this output was encoded from.
        pub rtp_timestamp: u32,
        /// Quantizer of the frame, or -1 when the encoder does not report it.
        pub qp: i32,
        /// Time from the start of the frame's encode to this output, in
        /// microseconds.
        pub encode_time_us: u32,
        /// Temporal layer of the frame, or -1 without temporal layers.
        pub temporal_id: i32,
    }

    /// Describes one decoded frame handed to [`VideoDecoderObserver`].
    #[derive(Debug, Clone, Copy)]
    pub struct DecodedVideoFrameInfo {
        /// Time the decoder took for the frame, in microseconds.
        pub decode_time_us: u32,
        /// Quantizer of the frame, or -1 when the decoder does not report it.
        pub qp: i32,
    }

    #[derive(Debug, Clone)]
    pub struct VideoCodecInfo {
        pub implementation_name: String,
        pub is_hardware_accelerated: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(i32)]
    pub enum VideoEncodeStatus {
        /// The frame was queued for encoding.
        Queued,
        /// The encoder is still busy with earlier frames; the frame was dropped.
        Busy,
        /// The frame cannot be encoded (e.g. it carries a pre-encoded buffer).
        InvalidFrame,
        /// An earlier frame failed to encode; `error_code` holds the
        /// `WEBRTC_VIDEO_CODEC_*` code. The current frame was not queued.
        EncoderError,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct VideoEncodeResult {
        pub status: VideoEncodeStatus,
        pub error_code: i32,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(i32)]
    pub enum VideoDecodeStatus {
        /// The frame was queued for decoding.
        Queued,
        /// The frame was not decoded and a keyframe is needed to resume.
        KeyframeRequired,
        /// The frame is empty.
        InvalidInput,
    }

    /// Describes the pre-encoded access unit carried by a native buffer.
    #[derive(Debug, Clone)]
    pub struct EncodedBufferInfo {
        /// `false` when the buffer does not carry a pre-encoded access unit,
        /// in which case the other fields are meaningless.
        pub is_encoded: bool,
        pub codec: EncodedVideoCodec,
        pub is_keyframe: bool,
        pub width: u32,
        pub height: u32,
    }

    extern "C++" {
        include!("livekit/webrtc.h");
        include!("livekit/video_track.h");
        include!("livekit/video_frame.h");
        include!("livekit/video_frame_buffer.h");

        type VideoEncoderBackend = crate::webrtc::ffi::VideoEncoderBackend;
        type EncodedVideoCodec = crate::video_track::ffi::EncodedVideoCodec;
        type VideoFrame = crate::video_frame::ffi::VideoFrame;
        type VideoFrameBuffer = crate::video_frame_buffer::ffi::VideoFrameBuffer;
    }

    unsafe extern "C++" {
        include!("livekit/video_codec.h");

        type NativeVideoEncoder;

        fn create_video_encoder(
            settings: &VideoEncoderSettings,
            observer: Box<VideoEncoderObserver>,
        ) -> Result<UniquePtr<NativeVideoEncoder>>;
        fn encode(
            self: &NativeVideoEncoder,
            frame: &VideoFrame,
            rtp_timestamp: u32,
        ) -> VideoEncodeResult;
        /// Forces a keyframe on `layer` (or every layer when negative) with
        /// the next encoded frame.
        fn request_keyframe(self: &NativeVideoEncoder, layer: i32);
        fn set_rates(self: &NativeVideoEncoder, layer_bitrates_bps: &[u32], framerate_fps: f64);
        fn set_layer_active(self: &NativeVideoEncoder, layer: u32, active: bool);
        fn info(self: &NativeVideoEncoder) -> VideoCodecInfo;

        type NativeVideoDecoder;

        /// With `prefer_software`, platform (hardware) decoders are skipped,
        /// e.g. to recover from a failing hardware decoder.
        fn create_video_decoder(
            codec: EncodedVideoCodec,
            prefer_software: bool,
            observer: Box<VideoDecoderObserver>,
        ) -> Result<UniquePtr<NativeVideoDecoder>>;
        fn decode(
            self: &NativeVideoDecoder,
            data: &[u8],
            is_keyframe: bool,
            rtp_timestamp: u32,
            width: u32,
            height: u32,
        ) -> VideoDecodeStatus;
        fn info(self: &NativeVideoDecoder) -> VideoCodecInfo;
        /// Frames the wrapped decoder failed to decode so far.
        fn decode_error_count(self: &NativeVideoDecoder) -> u64;

        fn encoded_buffer_info(buffer: &VideoFrameBuffer) -> EncodedBufferInfo;
        fn encoded_buffer_payload(buffer: &VideoFrameBuffer) -> &[u8];

        /// WebRTC's send-side processing of an H.264 keyframe (SPS VUI
        /// rewrite, AUDs dropped), as a WebRTC sender applies it.
        fn rewrite_h264_keyframe(data: &[u8]) -> Vec<u8>;

        /// Number of live native encoders and decoders.
        fn live_video_codec_count() -> i64;
    }

    extern "Rust" {
        type VideoEncoderObserver;

        fn on_encoded_frame(
            self: &VideoEncoderObserver,
            info: &EncodedVideoFrameInfo,
            data: &[u8],
        ) -> bool;

        type VideoDecoderObserver;

        fn on_decoded_frame(
            self: &VideoDecoderObserver,
            frame: UniquePtr<VideoFrame>,
            info: &DecodedVideoFrameInfo,
        );
    }
}

// SAFETY: every method of the native encoder and decoder only touches the
// wrapped codec on its own task queue, and the remaining state is atomic or
// mutex-guarded, so they can be shared across threads.
impl_thread_safety!(ffi::NativeVideoEncoder, Send + Sync);
impl_thread_safety!(ffi::NativeVideoDecoder, Send + Sync);

/// Receives the output of a [`ffi::NativeVideoEncoder`].
///
/// Called from encoder threads, possibly concurrently with other calls on the
/// encoder. Must not block, panic, or call back into the encoder.
pub trait EncodedVideoSink: Send + Sync {
    /// Returns `false` when the frame could not be accepted. The encoder then
    /// produces a keyframe on that layer next, since later frames of the
    /// layer may reference the dropped one.
    fn on_encoded_frame(&self, info: &ffi::EncodedVideoFrameInfo, data: &[u8]) -> bool;
}

/// Owned by the native encoder; forwards its output to an [`EncodedVideoSink`].
pub struct VideoEncoderObserver {
    sink: Box<dyn EncodedVideoSink>,
}

impl VideoEncoderObserver {
    pub fn new(sink: Box<dyn EncodedVideoSink>) -> Self {
        Self { sink }
    }

    fn on_encoded_frame(&self, info: &ffi::EncodedVideoFrameInfo, data: &[u8]) -> bool {
        self.sink.on_encoded_frame(info, data)
    }
}

/// Receives the output of a [`ffi::NativeVideoDecoder`].
///
/// Called from decoder threads. Must not block, panic, or call back into the
/// decoder.
pub trait DecodedVideoSink: Send + Sync {
    fn on_decoded_frame(&self, frame: UniquePtr<VideoFrame>, info: &ffi::DecodedVideoFrameInfo);
}

/// Owned by the native decoder; forwards its output to a [`DecodedVideoSink`].
pub struct VideoDecoderObserver {
    sink: Box<dyn DecodedVideoSink>,
}

impl VideoDecoderObserver {
    pub fn new(sink: Box<dyn DecodedVideoSink>) -> Self {
        Self { sink }
    }

    fn on_decoded_frame(&self, frame: UniquePtr<VideoFrame>, info: &ffi::DecodedVideoFrameInfo) {
        self.sink.on_decoded_frame(frame, info);
    }
}
