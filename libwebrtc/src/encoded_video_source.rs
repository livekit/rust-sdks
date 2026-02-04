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

use crate::sys;
use crate::video_source::VideoResolution;

/// Video codec type for encoded frames
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum VideoCodecType {
    VP8 = 1,
    VP9 = 2,
    AV1 = 3,
    H264 = 4,
    H265 = 5,
}

impl From<VideoCodecType> for sys::lkVideoCodecType {
    fn from(codec: VideoCodecType) -> Self {
        match codec {
            VideoCodecType::VP8 => sys::lkVideoCodecType::LK_VIDEO_CODEC_VP8,
            VideoCodecType::VP9 => sys::lkVideoCodecType::LK_VIDEO_CODEC_VP9,
            VideoCodecType::AV1 => sys::lkVideoCodecType::LK_VIDEO_CODEC_AV1,
            VideoCodecType::H264 => sys::lkVideoCodecType::LK_VIDEO_CODEC_H264,
            VideoCodecType::H265 => sys::lkVideoCodecType::LK_VIDEO_CODEC_H265,
        }
    }
}

/// Information about a pre-encoded video frame
#[derive(Debug, Clone)]
pub struct EncodedFrameInfo {
    /// The encoded frame data (e.g., H264 NALUs)
    pub data: Vec<u8>,
    /// Capture timestamp in microseconds
    pub capture_time_us: i64,
    /// RTP timestamp (set to 0 for auto-generation)
    pub rtp_timestamp: u32,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Whether this frame is a keyframe (IDR for H264)
    pub is_keyframe: bool,
    /// For H264: whether the frame includes SPS/PPS NALUs
    pub has_sps_pps: bool,
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
        }
    }
}

/// Callback trait for keyframe requests from the encoder
pub trait KeyFrameRequestCallback: Send + Sync {
    fn on_keyframe_request(&self);
}

/// A video source that accepts pre-encoded frames (H264, VP8, VP9, etc.)
///
/// This allows injecting pre-encoded video frames directly into the WebRTC pipeline,
/// bypassing the internal encoding step. Useful for:
/// - Streaming pre-encoded video files (IVF containers)
/// - Using external hardware encoders (NVENC, VideoToolbox output)
/// - Re-streaming already-encoded content
///
/// # Example
///
/// ```ignore
/// use libwebrtc::encoded_video_source::{EncodedVideoSource, EncodedFrameInfo, VideoCodecType};
///
/// // Create an encoded source for H264
/// let source = EncodedVideoSource::new(1920, 1080, VideoCodecType::H264);
///
/// // Create track and add to peer connection
/// let track = factory.create_video_track_from_encoded_source("video", &source)?;
/// peer_connection.add_track(&track, &["stream"]);
///
/// // Inject H264 frames
/// let frame_info = EncodedFrameInfo {
///     data: h264_nalu_data.to_vec(),
///     capture_time_us: timestamp_us,
///     rtp_timestamp: 0,  // auto-generate
///     width: 1920,
///     height: 1080,
///     is_keyframe: true,
///     has_sps_pps: true,
/// };
/// source.capture_frame(&frame_info);
/// ```
#[derive(Clone)]
pub struct EncodedVideoSource {
    pub(crate) ffi: sys::RefCounted<sys::lkEncodedVideoSource>,
    // Hold callback reference to prevent it from being dropped
    _callback: Option<Arc<dyn KeyFrameRequestCallback>>,
}

impl Debug for EncodedVideoSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncodedVideoSource")
            .field("resolution", &self.video_resolution())
            .finish()
    }
}

impl EncodedVideoSource {
    /// Create a new encoded video source
    ///
    /// # Arguments
    /// * `width` - The video width in pixels
    /// * `height` - The video height in pixels
    /// * `codec` - The video codec type (H264, VP8, etc.)
    pub fn new(width: u32, height: u32, codec: VideoCodecType) -> Self {
        let ffi = unsafe { sys::lkCreateEncodedVideoSource(width, height, codec.into()) };
        Self {
            ffi: unsafe { sys::RefCounted::from_raw(ffi) },
            _callback: None,
        }
    }

    /// Capture a pre-encoded frame
    ///
    /// This queues the encoded frame data and triggers the WebRTC encoding pipeline.
    /// The passthrough encoder will retrieve the queued data and emit it without re-encoding.
    ///
    /// # Arguments
    /// * `info` - Information about the encoded frame including the actual data
    ///
    /// # Returns
    /// `true` if the frame was successfully captured, `false` otherwise
    pub fn capture_frame(&self, info: &EncodedFrameInfo) -> bool {
        let ffi_info = sys::lkEncodedFrameInfo {
            data: info.data.as_ptr(),
            size: info.data.len() as u32,
            capture_time_us: info.capture_time_us,
            rtp_timestamp: info.rtp_timestamp,
            width: info.width,
            height: info.height,
            is_keyframe: info.is_keyframe,
            has_sps_pps: info.has_sps_pps,
        };
        unsafe { sys::lkEncodedVideoSourceCaptureFrame(self.ffi.as_ptr(), &ffi_info) }
    }

    /// Set a callback to be invoked when the encoder requests a keyframe
    ///
    /// This is typically called by the receiver when it needs a keyframe to
    /// recover from packet loss or to start decoding.
    ///
    /// # Arguments
    /// * `callback` - The callback to invoke on keyframe requests
    pub fn set_keyframe_request_callback(&mut self, callback: Arc<dyn KeyFrameRequestCallback>) {
        let callback_ptr = Arc::into_raw(callback.clone()) as *mut std::ffi::c_void;
        unsafe {
            sys::lkEncodedVideoSourceSetKeyFrameRequestCallback(
                self.ffi.as_ptr(),
                Some(keyframe_request_callback),
                callback_ptr,
            );
        }
        self._callback = Some(callback);
    }

    pub fn video_resolution(&self) -> VideoResolution {
        unsafe { sys::lkEncodedVideoSourceGetResolution(self.ffi.as_ptr()).into() }
    }

    pub fn codec_type(&self) -> VideoCodecType {
        let codec = unsafe { sys::lkEncodedVideoSourceGetCodecType(self.ffi.as_ptr()) };
        match codec {
            sys::lkVideoCodecType::LK_VIDEO_CODEC_VP8 => VideoCodecType::VP8,
            sys::lkVideoCodecType::LK_VIDEO_CODEC_VP9 => VideoCodecType::VP9,
            sys::lkVideoCodecType::LK_VIDEO_CODEC_AV1 => VideoCodecType::AV1,
            sys::lkVideoCodecType::LK_VIDEO_CODEC_H264 => VideoCodecType::H264,
            sys::lkVideoCodecType::LK_VIDEO_CODEC_H265 => VideoCodecType::H265,
            _ => VideoCodecType::H264,
        }
    }
}

extern "C" fn keyframe_request_callback(userdata: *mut std::ffi::c_void) {
    let callback = unsafe { &*(userdata as *const Arc<dyn KeyFrameRequestCallback>) };
    callback.on_keyframe_request();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_encoded_video_source() {
        let source = EncodedVideoSource::new(1920, 1080, VideoCodecType::H264);
        let resolution = source.video_resolution();
        assert_eq!(resolution.width, 1920);
        assert_eq!(resolution.height, 1080);
    }

    #[test]
    fn test_capture_frame() {
        let source = EncodedVideoSource::new(640, 480, VideoCodecType::H264);

        // Create a dummy H264 frame (just for testing the API)
        let frame_info = EncodedFrameInfo {
            data: vec![0u8; 100],
            capture_time_us: 1000000,
            rtp_timestamp: 0,
            width: 640,
            height: 480,
            is_keyframe: true,
            has_sps_pps: true,
        };

        let result = source.capture_frame(&frame_info);
        assert!(result);
    }
}
