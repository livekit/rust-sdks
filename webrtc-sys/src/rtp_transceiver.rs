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
    rtp_parameters::{RtpCodecCapability, RtpEncodingParameters, RtpTransceiverDirection},
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

impl From<sys::lkRtpTransceiverDirection> for RtpTransceiverDirection {
    fn from(state: sys::lkRtpTransceiverDirection) -> Self {
        match state {
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_SENDRECV => Self::SendRecv,

            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_SENDONLY => Self::SendOnly,
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_RECVONLY => Self::RecvOnly,

            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_INACTIVE => Self::Inactive,
            sys::lkRtpTransceiverDirection::LK_RTP_TRANSCEIVER_DIRECTION_STOPPED => Self::Stopped,
        }
    }
}

impl From<RtpTransceiverDirection> for sys::lkRtpTransceiverDirection {
    fn from(state: RtpTransceiverDirection) -> Self {
        match state {
            RtpTransceiverDirection::SendRecv => Self::LK_RTP_TRANSCEIVER_DIRECTION_SENDRECV,
            RtpTransceiverDirection::SendOnly => Self::LK_RTP_TRANSCEIVER_DIRECTION_SENDONLY,
            RtpTransceiverDirection::RecvOnly => Self::LK_RTP_TRANSCEIVER_DIRECTION_RECVONLY,
            RtpTransceiverDirection::Inactive => Self::LK_RTP_TRANSCEIVER_DIRECTION_INACTIVE,
            RtpTransceiverDirection::Stopped => Self::LK_RTP_TRANSCEIVER_DIRECTION_STOPPED,
        }
    }
}

#[derive(Clone)]
pub struct RtpTransceiver {
    pub ffi: sys::RefCounted<crate::sys::lkRtpTransceiver>,
}

impl RtpTransceiver {
    pub fn from_native(ffi: sys::RefCounted<crate::sys::lkRtpTransceiver>) -> Self {
        Self { ffi }
    }

    pub fn mid(&self) -> Option<String> {
        unsafe {
            let mid_ptr = sys::lkRtpTransceiverGetMid(self.ffi.as_ptr());
            if mid_ptr.is_null() {
                None
            } else {
                let ref_counted_str =
                    sys::RefCountedString { ffi: sys::RefCounted::from_raw(mid_ptr) };
                Some(ref_counted_str.as_str())
            }
        }
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        unsafe {
            let direction_ptr = sys::lkRtpTransceiverCurrentDirection(self.ffi.as_ptr());
            if direction_ptr.is_null() {
                None
            } else {
                Some((*direction_ptr).into())
            }
        }
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        unsafe { sys::lkRtpTransceiverGetDirection(self.ffi.as_ptr()).into() }
    }

    pub fn sender(&self) -> RtpSender {
        unsafe {
            let sender_ptr = sys::lkRtpTransceiverGetSender(self.ffi.as_ptr());
            RtpSender::from_native(unsafe { sys::RefCounted::from_raw(sender_ptr) })
        }
    }

    pub fn receiver(&self) -> RtpReceiver {
        unsafe {
            let receiver_ptr = sys::lkRtpTransceiverGetReceiver(self.ffi.as_ptr());
            RtpReceiver::from_native(unsafe { sys::RefCounted::from_raw(receiver_ptr) })
        }
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
