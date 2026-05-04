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

use std::sync::Arc;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    #[repr(u8)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EncodedVideoCodecType {
        H264 = 0,
        H265 = 1,
        Vp8 = 2,
        Vp9 = 3,
        Av1 = 4,
    }

    unsafe extern "C++" {
        include!("livekit/encoded_video_source.h");

        type EncodedVideoTrackSource;

        fn new_encoded_video_track_source(
            codec: EncodedVideoCodecType,
            width: u32,
            height: u32,
        ) -> SharedPtr<EncodedVideoTrackSource>;

        fn source_id(self: &EncodedVideoTrackSource) -> u16;
        fn codec(self: &EncodedVideoTrackSource) -> EncodedVideoCodecType;

        fn capture_frame(
            self: &EncodedVideoTrackSource,
            data: &[u8],
            is_keyframe: bool,
            has_sps_pps: bool,
            width: u32,
            height: u32,
            capture_time_us: i64,
        ) -> bool;

        fn set_observer(self: &EncodedVideoTrackSource, observer: Box<EncodedVideoSourceWrapper>);
    }

    extern "Rust" {
        type EncodedVideoSourceWrapper;

        fn on_keyframe_requested(self: &EncodedVideoSourceWrapper);
        fn on_target_bitrate(
            self: &EncodedVideoSourceWrapper,
            bitrate_bps: u32,
            framerate_fps: f64,
        );
    }
}

impl_thread_safety!(ffi::EncodedVideoTrackSource, Send + Sync);

/// Trait implemented by Rust consumers to receive encoder feedback (keyframe
/// requests, target bitrate updates) from WebRTC.
pub trait EncodedVideoSourceObserver: Send + Sync {
    fn on_keyframe_requested(&self);
    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64);
}

pub struct EncodedVideoSourceWrapper {
    observer: Arc<dyn EncodedVideoSourceObserver>,
}

impl EncodedVideoSourceWrapper {
    pub fn new(observer: Arc<dyn EncodedVideoSourceObserver>) -> Self {
        Self { observer }
    }

    fn on_keyframe_requested(&self) {
        self.observer.on_keyframe_requested();
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        self.observer.on_target_bitrate(bitrate_bps, framerate_fps);
    }
}
