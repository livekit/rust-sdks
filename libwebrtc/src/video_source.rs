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

use livekit_protocol::enum_dispatch;

use crate::imp::video_source as vs_imp;

#[derive(Debug, Clone)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

impl Default for VideoResolution {
    // Default to 720p
    fn default() -> Self {
        VideoResolution { width: 1280, height: 720 }
    }
}

/// Codec used by an encoded video feed.
#[cfg(feature = "encoded-video")]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

/// Metadata describing a single encoded video frame pushed to an
/// [`native::NativeEncodedVideoSource`].
#[cfg(feature = "encoded-video")]
#[derive(Debug, Copy, Clone)]
pub struct EncodedFrameInfo {
    /// True when this frame is an IDR / keyframe.
    pub is_keyframe: bool,
    /// True when the `data` buffer already has SPS/PPS (or equivalent)
    /// prepended. H.264/H.265 only; ignored for other codecs.
    pub has_sps_pps: bool,
    pub width: u32,
    pub height: u32,
    /// Capture timestamp in microseconds. `0` lets the source stamp `now`.
    pub capture_time_us: i64,
}

#[cfg(feature = "encoded-video")]
impl Default for EncodedFrameInfo {
    fn default() -> Self {
        Self { is_keyframe: false, has_sps_pps: false, width: 0, height: 0, capture_time_us: 0 }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcVideoSource {
    // TODO(theomonnom): Web video sources (eq. to tracks on browsers?)
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeVideoSource),
    #[cfg(all(not(target_arch = "wasm32"), feature = "encoded-video"))]
    Encoded(native::NativeEncodedVideoSource),
}

// TODO(theomonnom): Support enum dispatch with conditional compilation?
#[cfg(all(not(target_arch = "wasm32"), feature = "encoded-video"))]
impl RtcVideoSource {
    enum_dispatch!(
        [Native, Encoded];
        pub fn video_resolution(self: &Self) -> VideoResolution;
    );
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "encoded-video")))]
impl RtcVideoSource {
    enum_dispatch!(
        [Native];
        pub fn video_resolution(self: &Self) -> VideoResolution;
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    #[cfg(feature = "encoded-video")]
    pub use crate::native::encoded_video_source::{
        EncodedVideoSourceObserver, NativeEncodedVideoSource,
    };

    use super::*;
    use crate::native::packet_trailer::PacketTrailerHandler;
    use crate::video_frame::{VideoBuffer, VideoFrame};

    #[derive(Clone)]
    pub struct NativeVideoSource {
        pub(crate) handle: vs_imp::NativeVideoSource,
    }

    impl Debug for NativeVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSource").finish()
        }
    }

    impl Default for NativeVideoSource {
        fn default() -> Self {
            Self::new(VideoResolution::default(), false)
        }
    }

    impl NativeVideoSource {
        pub fn new(resolution: VideoResolution, is_screencast: bool) -> Self {
            Self { handle: vs_imp::NativeVideoSource::new(resolution, is_screencast) }
        }

        pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }

        /// Set the packet trailer handler used by this source.
        ///
        /// When set, any frame captured with a `user_timestamp` value will
        /// automatically have its timestamp stored in the handler (keyed by
        /// the TimestampAligner-adjusted capture timestamp) so the
        /// `PacketTrailerTransformer` can embed it into the encoded frame.
        pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
            self.handle.set_packet_trailer_handler(handler)
        }

        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
