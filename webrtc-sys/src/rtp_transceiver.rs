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
    rtp_parameters::{RtpCodecCapability, RtpEncodingParameters},
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
    sys, RtcError,
};

#[derive(Debug, Clone)]
pub struct RtpTransceiverInit {
    pub direction: RtpTransceiverDirection,
    pub stream_ids: Vec<String>,
    pub send_encodings: Vec<RtpEncodingParameters>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtpTransceiverDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
    Stopped,
}

#[derive(Clone)]
pub struct RtpTransceiver {
    pub(crate) ffi: sys::RefCounted<crate::sys::lkRtpTransceiver>,
}

impl RtpTransceiver {
    pub fn mid(&self) -> Option<String> {
        todo!()
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        todo!()
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        todo!()
    }

    pub fn sender(&self) -> RtpSender {
        todo!()
    }

    pub fn receiver(&self) -> RtpReceiver {
        todo!()
    }

    pub fn set_codec_preferences(&self, codecs: Vec<RtpCodecCapability>) -> Result<(), RtcError> {
        todo!()
    }

    pub fn stop(&self) -> Result<(), RtcError> {
        todo!()
    }
}

impl Debug for RtpTransceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpTransceiver")
            .field("mid", &self.mid())
            .field("direction", &self.direction())
            .field("sender", &self.sender())
            .field("receiver", &self.receiver())
            .finish()
    }
}
