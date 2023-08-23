// Copyright 2023 LiveKit, Inc.
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

use crate::imp::media_stream_track::new_media_stream_track;
use crate::media_stream_track::MediaStreamTrack;
use crate::rtp_parameters::RtpParameters;
use cxx::SharedPtr;
use webrtc_sys::frame_transformer::EncodedFrameSinkWrapper;
use webrtc_sys::frame_transformer::SenderReportSinkWrapper;
use webrtc_sys::frame_transformer::ffi::AdaptedNativeFrameTransformer;
use webrtc_sys::frame_transformer::ffi::AdaptedNativeSenderReportCallback;
use webrtc_sys::rtp_receiver as sys_rr;
use webrtc_sys::frame_transformer as sys_ft;

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) sys_handle: SharedPtr<sys_rr::ffi::RtpReceiver>,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        println!("RtpReceiver::track()");
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub fn parameters(&self) -> RtpParameters {
        println!("RtpReceiver::parameters()");
        self.sys_handle.get_parameters().into()
    }

    // frame_transformer: SharedPtr<FrameTransformer>
    pub fn set_depacketizer_to_decoder_frame_transformer(&self, transformer:  SharedPtr<AdaptedNativeFrameTransformer>) {
        self.sys_handle.set_depacketizer_to_decoder_frame_transformer(transformer);
    }

    pub fn new_adapted_frame_transformer(&self, observer: Box<EncodedFrameSinkWrapper>, is_video: bool) -> SharedPtr<AdaptedNativeFrameTransformer> {
        sys_ft::ffi::new_adapted_frame_transformer(observer, is_video)
    }

    pub fn set_sender_report_callback(&self, callback:  SharedPtr<AdaptedNativeSenderReportCallback>) {
        self.sys_handle.set_sender_report_callback(callback);
    }

    pub fn new_adapted_sender_report_callback(&self, observer: Box<SenderReportSinkWrapper>) -> SharedPtr<AdaptedNativeSenderReportCallback> {
        sys_ft::ffi::new_adapted_sender_report_callback(observer)
    }

    pub fn request_key_frame(&self) {
        self.sys_handle.request_key_frame();
    }
}
