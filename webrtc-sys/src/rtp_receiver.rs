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

use crate::{
    media_stream_track::MediaStreamTrack, rtp_parameters::RtpParameters, stats::RtcStats, sys,
    RtcError,
};

#[derive(Clone)]
pub struct RtpReceiver {
    pub ffi: sys::RefCounted<crate::sys::lkRtpReceiver>,
}

impl RtpReceiver {
    pub fn from_native(ffi: sys::RefCounted<sys::lkRtpReceiver>) -> Self {
        Self { ffi }
    }

    pub fn track(&self) -> Option<MediaStreamTrack> {
        unsafe {
            let track_ptr = sys::lkRtpReceiverGetTrack(self.ffi.as_ptr());
            if track_ptr.is_null() {
                None
            } else {
                Some(crate::media_stream_track::new_media_stream_track(unsafe {
                    sys::RefCounted::from_raw(track_ptr)
                }))
            }
        }
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        todo!()
    }

    pub fn parameters(&self) -> RtpParameters {
        todo!()
    }
}

impl Debug for RtpReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("track", &self.track())
            .field("cname", &self.parameters().rtcp.cname)
            .finish()
    }
}
