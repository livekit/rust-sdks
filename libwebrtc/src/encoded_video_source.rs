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

//! Pre-encoded video source for injecting H.264/H.265/VP8/VP9/AV1 frames
//! directly into the WebRTC RTP pipeline.
//!
//! Use this when you already have an encoded bitstream (e.g. from a hardware
//! capture device, a transcoder, or a network feed) and want to publish it
//! through LiveKit without re-encoding.
//!
//! See [`native::NativeEncodedVideoSource`] for the entry point and
//! `examples/preencoded_ingest` for an end-to-end example.

/// Codec used for the pre-encoded payloads pushed through a
/// [`native::NativeEncodedVideoSource`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VideoCodecType {
    VP8 = 1,
    VP9 = 2,
    AV1 = 3,
    H264 = 4,
    H265 = 5,
}

/// A single pre-encoded frame payload to push through the source.
///
/// Width/height come from the source itself (set at construction).  Capture
/// timestamp + frame id are tracked in [`crate::native::FrameMetadata`] and
/// auto-filled by `capture_frame` when not provided explicitly.
#[derive(Debug, Clone)]
pub struct EncodedFrameInfo {
    /// Encoded payload (Annex-B for H.264/H.265, IVF-stripped for VPx, etc.).
    pub data: Vec<u8>,
    /// Whether this frame is a keyframe (IDR for H.264).
    pub is_keyframe: bool,
    /// Whether the payload includes the codec parameter sets (SPS/PPS for
    /// H.264, VPS/SPS/PPS for H.265).  Informational; the encoder forwards
    /// the bytes as-is.
    pub has_sps_pps: bool,
}

impl EncodedFrameInfo {
    /// Construct a delta frame from the given payload.
    pub fn delta(data: Vec<u8>) -> Self {
        Self { data, is_keyframe: false, has_sps_pps: false }
    }

    /// Construct a keyframe (with parameter sets included) from the given
    /// payload.
    pub fn keyframe(data: Vec<u8>) -> Self {
        Self { data, is_keyframe: true, has_sps_pps: true }
    }
}

/// Receives keyframe requests (PLI/FIR) raised by the remote peer.
///
/// Implement this and pass it to
/// [`native::NativeEncodedVideoSource::set_keyframe_request_callback`] to
/// be notified when an upstream encoder needs to emit a fresh IDR.
pub trait KeyFrameRequestCallback: Send + Sync {
    fn on_keyframe_request(&self);
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::{
        fmt::{Debug, Formatter},
        sync::Arc,
    };

    use super::*;
    use crate::imp::encoded_video_source as evs_imp;
    use crate::native::packet_trailer::PacketTrailerHandler;
    use crate::video_source::VideoResolution;

    /// Frame metadata mirrored from the raw video pipeline.
    ///
    /// Constructed manually when the caller needs precise control over
    /// `user_timestamp` / `frame_id`; otherwise
    /// [`NativeEncodedVideoSource::capture_frame`] auto-fills sensible
    /// defaults.
    #[derive(Debug, Clone, Copy)]
    pub struct FrameMetadata {
        /// If `true` and a [`PacketTrailerHandler`] has been set on the
        /// source, the trailer transformer will embed `user_timestamp` and
        /// `frame_id` into the egress RTP packets.
        pub has_packet_trailer: bool,
        /// Caller-defined timestamp embedded in the packet trailer.
        pub user_timestamp: u64,
        /// Monotonically increasing frame identifier embedded in the packet
        /// trailer.
        pub frame_id: u32,
    }

    /// A pre-encoded video source that drives the WebRTC RTP pipeline via a
    /// paired passthrough encoder.
    ///
    /// Each [`Self::capture_frame`] call enqueues the payload, kicks the
    /// internal encoder, and forwards the bytes unchanged onto the RTP
    /// sender.  The source is `Clone` -- the underlying handle is shared
    /// across clones.
    #[derive(Clone)]
    pub struct NativeEncodedVideoSource {
        pub(crate) handle: evs_imp::NativeEncodedVideoSource,
    }

    impl Debug for NativeEncodedVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeEncodedVideoSource")
                .field("resolution", &self.handle.video_resolution())
                .field("codec", &self.handle.codec_type())
                .finish()
        }
    }

    impl NativeEncodedVideoSource {
        /// Build a new source advertising the given resolution + codec.
        ///
        /// The codec selection determines which SDP format the encoder
        /// factory will route to the underlying passthrough encoder.
        pub fn new(width: u32, height: u32, codec: VideoCodecType) -> Self {
            Self { handle: evs_imp::NativeEncodedVideoSource::new(width, height, codec) }
        }

        /// Push a pre-encoded frame onto the track.
        ///
        /// Auto-fills [`FrameMetadata`] with `user_timestamp` set to the
        /// current system time (microseconds since epoch) and `frame_id`
        /// taken from a per-source monotonic counter.  Use
        /// [`Self::capture_frame_with_metadata`] when the caller needs
        /// explicit control.
        pub fn capture_frame(&self, info: &EncodedFrameInfo) -> bool {
            self.handle.capture_frame(info)
        }

        /// Push a pre-encoded frame with caller-supplied metadata.
        pub fn capture_frame_with_metadata(
            &self,
            info: &EncodedFrameInfo,
            metadata: &FrameMetadata,
        ) -> bool {
            self.handle.capture_frame_with_metadata(info, metadata)
        }

        /// Register a callback fired when the remote peer requests a
        /// keyframe (PLI/FIR).  Replaces any previously registered
        /// callback.
        pub fn set_keyframe_request_callback(
            &self,
            callback: Arc<dyn KeyFrameRequestCallback>,
        ) {
            self.handle.set_keyframe_request_callback(callback);
        }

        /// Attach a [`PacketTrailerHandler`] so frames captured with
        /// `has_packet_trailer = true` get their metadata embedded into
        /// the egress RTP packets.
        pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
            self.handle.set_packet_trailer_handler(handler);
        }

        /// Resolution declared at construction.
        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }

        /// Codec declared at construction.
        pub fn codec_type(&self) -> VideoCodecType {
            self.handle.codec_type()
        }
    }
}
