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

use cxx::SharedPtr;
use webrtc_sys::encoded_video_source as evs_sys;

use crate::{
    encoded_video_source::{EncodedFrameInfo, KeyFrameRequestCallback, VideoCodecType},
    video_source::VideoResolution,
};

impl From<VideoCodecType> for evs_sys::ffi::VideoCodecType {
    fn from(codec: VideoCodecType) -> Self {
        match codec {
            VideoCodecType::VP8 => Self::VP8,
            VideoCodecType::VP9 => Self::VP9,
            VideoCodecType::AV1 => Self::AV1,
            VideoCodecType::H264 => Self::H264,
            VideoCodecType::H265 => Self::H265,
        }
    }
}

impl From<evs_sys::ffi::VideoCodecType> for VideoCodecType {
    fn from(codec: evs_sys::ffi::VideoCodecType) -> Self {
        match codec {
            evs_sys::ffi::VideoCodecType::VP8 => Self::VP8,
            evs_sys::ffi::VideoCodecType::VP9 => Self::VP9,
            evs_sys::ffi::VideoCodecType::AV1 => Self::AV1,
            evs_sys::ffi::VideoCodecType::H264 => Self::H264,
            evs_sys::ffi::VideoCodecType::H265 => Self::H265,
            _ => Self::H264,
        }
    }
}

#[derive(Clone)]
pub struct NativeEncodedVideoSource {
    sys_handle: SharedPtr<evs_sys::ffi::EncodedVideoTrackSource>,
}

impl NativeEncodedVideoSource {
    pub fn new(width: u32, height: u32, codec: VideoCodecType) -> Self {
        Self {
            sys_handle: evs_sys::ffi::new_encoded_video_track_source(
                width,
                height,
                codec.into(),
            ),
        }
    }

    pub fn sys_handle(&self) -> SharedPtr<evs_sys::ffi::EncodedVideoTrackSource> {
        self.sys_handle.clone()
    }

    pub fn capture_frame(&self, info: &EncodedFrameInfo) -> bool {
        evs_sys::ffi::capture_encoded_frame(
            &self.sys_handle,
            &info.data,
            info.capture_time_us,
            info.rtp_timestamp,
            info.width,
            info.height,
            info.is_keyframe,
            info.has_sps_pps,
            info.simulcast_index,
        )
    }

    pub fn set_keyframe_request_callback(&self, callback: Arc<dyn KeyFrameRequestCallback>) {
        struct CallbackAdapter(Arc<dyn KeyFrameRequestCallback>);

        impl evs_sys::KeyFrameRequestObserver for CallbackAdapter {
            fn on_keyframe_request(&self) {
                self.0.on_keyframe_request();
            }
        }

        let wrapper = evs_sys::KeyFrameRequestObserverWrapper::new(Arc::new(CallbackAdapter(
            callback,
        )));
        self.sys_handle.set_keyframe_request_callback(Box::new(wrapper));
    }

    pub fn video_resolution(&self) -> VideoResolution {
        self.sys_handle.video_resolution().into()
    }

    pub fn codec_type(&self) -> VideoCodecType {
        self.sys_handle.codec_type().into()
    }
}
