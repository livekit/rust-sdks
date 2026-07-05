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

use crate::{enum_dispatch, imp::video_source as vs_imp};

#[derive(Debug, Clone)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

/// Encoder rate-control target requested by WebRTC for a pre-encoded source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncodedRateControl {
    /// Target bitrate in bits per second.
    pub target_bitrate_bps: u64,
    /// Target frame rate in frames per second.
    pub framerate_fps: f64,
}

impl Default for VideoResolution {
    // Default to 720p
    fn default() -> Self {
        VideoResolution { width: 1280, height: 720 }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcVideoSource {
    // TODO(theomonnom): Web video sources (eq. to tracks on browsers?)
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeVideoSource),
}

// TODO(theomonnom): Support enum dispatch with conditional compilation?
impl RtcVideoSource {
    enum_dispatch!(
        [Native];
        pub fn video_resolution(self: &Self) -> VideoResolution;
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    use super::*;
    use crate::native::packet_trailer::PacketTrailerHandler;
    #[cfg(target_os = "linux")]
    use crate::video_frame::FrameMetadata;
    use crate::video_frame::{EncodedVideoFrame, VideoBuffer, VideoFrame};

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

        /// Creates a source for pre-encoded access units: no raw black-frame
        /// keepalive is injected before the first capture.
        pub fn new_encoded(resolution: VideoResolution) -> Self {
            Self { handle: vs_imp::NativeVideoSource::new_encoded(resolution) }
        }

        pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }

        /// Captures one pre-encoded video access unit.
        pub fn capture_encoded_frame(&self, frame: &EncodedVideoFrame<'_>) -> bool {
            self.handle.capture_encoded_frame(frame)
        }

        /// Returns and clears the pending keyframe request raised by the
        /// pass-through encoder (PLI/FIR or reconfiguration).
        pub fn take_keyframe_request(&self) -> bool {
            self.handle.take_keyframe_request()
        }

        /// Returns and clears the pending rate-control target raised by the
        /// pass-through encoder.
        pub fn take_rate_control_request(&self) -> Option<EncodedRateControl> {
            self.handle.take_rate_control_request()
        }

        /// Captures a Jetson DMA-buffer backed video frame.
        ///
        /// `pixel_format` is `0` for NV12 and `1` for YUV420M.
        #[cfg(target_os = "linux")]
        pub fn capture_dmabuf_frame(
            &self,
            dmabuf_fd: i32,
            width: u32,
            height: u32,
            pixel_format: i32,
            timestamp_us: i64,
        ) -> bool {
            self.handle.capture_dmabuf_frame(dmabuf_fd, width, height, pixel_format, timestamp_us)
        }

        /// Captures a Jetson DMA-buffer backed video frame with packet trailer metadata.
        ///
        /// `pixel_format` is `0` for NV12 and `1` for YUV420M.
        #[cfg(target_os = "linux")]
        pub fn capture_dmabuf_frame_with_metadata(
            &self,
            dmabuf_fd: i32,
            width: u32,
            height: u32,
            pixel_format: i32,
            timestamp_us: i64,
            frame_metadata: Option<FrameMetadata>,
        ) -> bool {
            self.handle.capture_dmabuf_frame_with_metadata(
                dmabuf_fd,
                width,
                height,
                pixel_format,
                timestamp_us,
                frame_metadata,
            )
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
