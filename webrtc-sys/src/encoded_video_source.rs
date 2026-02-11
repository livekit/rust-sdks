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

use std::sync::Arc;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(i32)]
    pub enum VideoCodecType {
        VP8 = 1,
        VP9 = 2,
        AV1 = 3,
        H264 = 4,
        H265 = 5,
    }

    extern "C++" {
        include!("livekit/video_track.h");

        type VideoResolution = crate::video_track::ffi::VideoResolution;
    }

    unsafe extern "C++" {
        include!("livekit/encoded_video_source.h");

        type EncodedVideoTrackSource;

        fn video_resolution(self: &EncodedVideoTrackSource) -> VideoResolution;
        fn codec_type(self: &EncodedVideoTrackSource) -> VideoCodecType;

        fn capture_encoded_frame(
            source: &EncodedVideoTrackSource,
            data: &[u8],
            capture_time_us: i64,
            rtp_timestamp: u32,
            width: u32,
            height: u32,
            is_keyframe: bool,
            has_sps_pps: bool,
        ) -> bool;

        fn set_keyframe_request_callback(
            self: &EncodedVideoTrackSource,
            observer: Box<KeyFrameRequestObserverWrapper>,
        );

        fn new_encoded_video_track_source(
            width: u32,
            height: u32,
            codec: VideoCodecType,
        ) -> SharedPtr<EncodedVideoTrackSource>;

        fn _shared_encoded_video_track_source() -> SharedPtr<EncodedVideoTrackSource>;
    }

    extern "Rust" {
        type KeyFrameRequestObserverWrapper;

        fn on_keyframe_request(self: &KeyFrameRequestObserverWrapper);
    }
}

// Ensure CXX generates SharedPtr support
// (the dummy function in C++ returns nullptr)

impl_thread_safety!(ffi::EncodedVideoTrackSource, Send + Sync);

pub trait KeyFrameRequestObserver: Send + Sync {
    fn on_keyframe_request(&self);
}

pub struct KeyFrameRequestObserverWrapper {
    observer: Arc<dyn KeyFrameRequestObserver>,
}

impl KeyFrameRequestObserverWrapper {
    pub fn new(observer: Arc<dyn KeyFrameRequestObserver>) -> Self {
        Self { observer }
    }

    fn on_keyframe_request(&self) {
        self.observer.on_keyframe_request();
    }
}
