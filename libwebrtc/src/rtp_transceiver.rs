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
    rtp_parameters::{RtpCodecCapability, RtpTransceiverDirection},
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
    sys, RtcError,
};

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
            let mut native_codecs = sys::RefCountedVector::new();

            for c in codecs {
                let mime_type_cstr = std::ffi::CString::new(c.mime_type.clone()).unwrap();
                let cap = sys::lkRtpCodecCapabilityCreate();

                if let Some(ch) = c.channels {
                    sys::lkRtpCodecCapabilitySetChannels(cap, ch.try_into().unwrap());
                }

                if let Some(sdp_fmtp_line) = &c.sdp_fmtp_line {
                    let str =   std::ffi::CString::new(sdp_fmtp_line.clone()).unwrap();
                    sys::lkRtpCodecCapabilitySetSdpFmtpLine(
                        cap,
                        str.as_ptr(),
                    );
                }

                if let Some(clock_rate) = c.clock_rate {
                    sys::lkRtpCodecCapabilitySetClockRate(cap, clock_rate.try_into().unwrap());
                }

                sys::lkRtpCodecCapabilitySetMimeType(cap, mime_type_cstr.as_ptr());
        
                if let Some(payload_type) = c.preferred_payload_type {
                    sys::lkRtpCodecCapabilitySetPreferredPayloadType(cap, payload_type as i32);
                }

                let mut rtcp_feedbacks = sys::RefCountedVector::new();

                for fb in c.rtcp_feedback {
                    let rtcp_fb = sys::lkRtcpFeedbackCreate(
                        fb.feedback_type.into(),
                        fb.has_message_type,
                        fb.message_type.into(),
                    );
                    rtcp_feedbacks.push_back(sys::RefCounted::from_raw(
                        rtcp_fb as *mut sys::lkRefCountedObject,
                    ));
                }

                sys::lkRtpCodecCapabilitySetRtcpFeedbacks(cap, rtcp_feedbacks.ffi.as_ptr());

                native_codecs
                    .push_back(sys::RefCounted::from_raw(cap as *mut sys::lkRefCountedObject));
            }

            let mut lk_err = sys::lkRtcError { message: std::ptr::null() };

            if !sys::lkRtpTransceiverSetCodecPreferences(
                self.ffi.as_ptr(),
                native_codecs.ffi.as_ptr(),
                &mut lk_err,
            ) {
                //TODO handle error
                return Err(RtcError {
                    error_type: crate::RtcErrorType::Internal,
                    message: "set_codec_preferences failed".to_owned(),
                });
            }
            Ok(())
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
