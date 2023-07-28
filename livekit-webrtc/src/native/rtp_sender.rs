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

use super::media_stream_track::new_media_stream_track;
use crate::media_stream_track::MediaStreamTrack;
use crate::{rtp_parameters::RtpParameters, RtcError, RtcErrorType};
use cxx::SharedPtr;
use webrtc_sys::rtc_error as sys_err;
use webrtc_sys::rtp_sender as sys_rs;

#[derive(Clone)]
pub struct RtpSender {
    pub(crate) sys_handle: SharedPtr<sys_rs::ffi::RtpSender>,
}

impl RtpSender {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        if !self
            .sys_handle
            .set_track(track.map_or(SharedPtr::null(), |t| t.sys_handle()))
        {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "Failed to set track".to_string(),
            });
        }

        Ok(())
    }

    pub fn parameters(&self) -> RtpParameters {
        self.sys_handle.get_parameters().into()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        self.sys_handle
            .set_parameters(parameters.into())
            .map_err(|e| unsafe { sys_err::ffi::RtcError::from(e.what()).into() })
    }
}
