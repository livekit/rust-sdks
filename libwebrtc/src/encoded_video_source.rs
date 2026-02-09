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

use std::fmt::Debug;
use std::sync::Arc;

use crate::video_source::VideoResolution;

/// Video codec type for encoded frames.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VideoCodecType {
    VP8 = 1,
    VP9 = 2,
    AV1 = 3,
    H264 = 4,
    H265 = 5,
}

/// Information about a pre-encoded video frame.
#[derive(Debug, Clone)]
pub struct EncodedFrameInfo {
    /// The encoded frame data (e.g., H264 NALUs in Annex B format).
    pub data: Vec<u8>,
    /// Capture timestamp in microseconds.
    pub capture_time_us: i64,
    /// RTP timestamp (set to 0 for auto-generation).
    pub rtp_timestamp: u32,
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Whether this frame is a keyframe (IDR for H264).
    pub is_keyframe: bool,
    /// For H264: whether the frame includes SPS/PPS NALUs.
    pub has_sps_pps: bool,
    /// Simulcast layer index (0 = lowest quality / single layer).
    /// When publishing simulcast, use 0 for `q`, 1 for `h`, 2 for `f`.
    pub simulcast_index: u32,
}

impl Default for EncodedFrameInfo {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            capture_time_us: 0,
            rtp_timestamp: 0,
            width: 0,
            height: 0,
            is_keyframe: false,
            has_sps_pps: false,
            simulcast_index: 0,
        }
    }
}

/// Callback trait for keyframe requests from the encoder/receiver.
pub trait KeyFrameRequestCallback: Send + Sync {
    fn on_keyframe_request(&self);
}

/// A video source that accepts pre-encoded frames (H264, VP8, VP9, etc.)
///
/// This allows injecting pre-encoded video frames directly into the WebRTC
/// pipeline, bypassing the internal encoding step.
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};
    use std::sync::Arc;

    use super::*;
    use crate::imp::encoded_video_source as evs_imp;

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
        pub fn new(width: u32, height: u32, codec: VideoCodecType) -> Self {
            Self {
                handle: evs_imp::NativeEncodedVideoSource::new(width, height, codec),
            }
        }

        pub fn capture_frame(&self, info: &EncodedFrameInfo) -> bool {
            self.handle.capture_frame(info)
        }

        pub fn set_keyframe_request_callback(
            &mut self,
            callback: Arc<dyn KeyFrameRequestCallback>,
        ) {
            self.handle.set_keyframe_request_callback(callback);
        }

        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }

        pub fn codec_type(&self) -> VideoCodecType {
            self.handle.codec_type()
        }
    }
}
