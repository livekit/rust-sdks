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

use crate::media_stream_track::new_media_stream_track;
use crate::rtp_parameters::RtpParameters;
use crate::stats::RtcStats;
use crate::{media_stream_track::MediaStreamTrack, sys, RtcError};
use std::fmt::Debug;

#[derive(Clone)]
pub struct RtpSender {
    pub ffi: sys::RefCounted<sys::lkRtpSender>,
}

impl RtpSender {
    pub fn from_native(ffi: sys::RefCounted<sys::lkRtpSender>) -> Self {
        Self { ffi }
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        todo!()
    }

    pub fn track(&self) -> Option<MediaStreamTrack> {
        unsafe {
            let track_ptr = sys::lkRtpSenderGetTrack(self.ffi.as_ptr());
            if track_ptr.is_null() {
                None
            } else {
                Some(new_media_stream_track(unsafe { sys::RefCounted::from_raw(track_ptr) }))
            }
        }
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        unsafe {
            let track_ptr = match track {
                Some(t) => t.ffi().as_ptr(),
                None => std::ptr::null_mut(),
            };
            let result = sys::lkRtpSenderSetTrack(self.ffi.as_ptr(), track_ptr);
            if !result {
                return Err(RtcError {
                    error_type: crate::RtcErrorType::Internal,
                    message: "Failed to set track".to_string(),
                });
            }
            Ok(())
        }
    }

    pub fn parameters(&self) -> RtpParameters {
        todo!()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        todo!()
    }
}

impl Debug for RtpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpReceiver").field("cname", &self.parameters().rtcp.cname).finish()
    }
}
