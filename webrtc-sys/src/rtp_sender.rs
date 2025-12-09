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
use crate::rtp_parameters::RtpParameters;
use crate::stats::RtcStats;
use crate::{media_stream_track::MediaStreamTrack, sys, RtcError};

#[derive(Clone)]
pub(crate) struct RtpSender {
    pub(crate) ffi: sys::RefCounted<sys::lkRtpSender>,
}

impl RtpSender {
    pub(crate) fn from_native(ffi: sys::RefCounted<sys::lkRtpSender>) -> Self {
        Self { ffi }
    }

    pub async fn get_stats(&self) -> Result<Vec<RtcStats>, RtcError> {
        todo!()
    }

    pub fn track(&self) -> Option<MediaStreamTrack> {
        todo!()
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        todo!()
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
