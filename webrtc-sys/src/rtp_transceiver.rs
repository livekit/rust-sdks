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
            Some(direction_ptr.into())
        }
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        unsafe { sys::lkRtpTransceiverGetDirection(self.ffi.as_ptr()).into() }
    }

    pub fn sender(&self) -> RtpSender {
        unsafe {
            let sender_ptr = sys::lkRtpTransceiverGetSender(self.ffi.as_ptr());
            RtpSender::from_native(sys::RefCounted::from_raw(sender_ptr))
        }
    }

    pub fn receiver(&self) -> RtpReceiver {
        unsafe {
            let receiver_ptr = sys::lkRtpTransceiverGetReceiver(self.ffi.as_ptr());
            RtpReceiver::from_native(sys::RefCounted::from_raw(receiver_ptr))
        }
    }

    pub fn set_codec_preferences(&self, codecs: Vec<RtpCodecCapability>) -> Result<(), RtcError> {
        unsafe {
            let mut error: sys::RefCounted<sys::lkRtcError> =
                sys::RefCounted::from_raw(std::ptr::null_mut());

            let mut native_codecs = sys::RefCountedVector::new();

            for c in codecs {
                let mime_type_cstr = std::ffi::CString::new(c.mime_type.clone()).unwrap();
                let sdp_fmtp_line_cstr = match &c.sdp_fmtp_line {
                    Some(line) => std::ffi::CString::new(line.clone()).unwrap(),
                    None => std::ffi::CString::new("").unwrap(),
                };
                let cap = sys::lkRtpCodecCapabilityCreate();

                sys::lkRtpCodecCapabilitySetChannels(cap, c.channels.unwrap_or(1));
                sys::lkRtpCodecCapabilitySetClockRate(
                    cap,
                    c.clock_rate.unwrap_or(0).try_into().unwrap(),
                );
                sys::lkRtpCodecCapabilitySetMimeType(cap, mime_type_cstr.as_ptr());

                sys::lkRtpCodecCapabilitySetSdpFmtpLine(cap, sdp_fmtp_line_cstr.as_ptr());

                native_codecs
                    .push_back(sys::RefCounted::from_raw(cap as *mut sys::lkRefCountedObject));
            }

            sys::lkRtpTransceiverSetCodecPreferences(
                self.ffi.as_ptr(),
                native_codecs.ffi.as_ptr(),
                error.as_ptr(),
            );
            if !error.is_null() {
                //TODO handle error
                Err(RtcError {
                    error_type: crate::RtcErrorType::Internal,
                    message: "set_codec_preferences failed".to_owned(),
                })
            } else {
                Ok(())
            }
        }
    }

    pub fn stop(&self) -> Result<(), RtcError> {
        unsafe {
            sys::lkRtpTransceiverStop(self.ffi.as_ptr());
            //TODO: check for errors
            Ok(())
        }
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
