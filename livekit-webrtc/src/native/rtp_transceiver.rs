use crate::prelude::*;
use cxx::SharedPtr;
use std::fmt::{Debug, Formatter};
use webrtc_sys::rtp_transceiver as sys_rt;

#[derive(Debug)]
pub struct RtpTransceiverInit {
    pub direction: RtpTransceiverDirection,
    pub stream_ids: Vec<String>,
    pub send_encodings: Vec<RtpEncodingParameters>,
}

impl From<RtpTransceiverInit> for sys_rt::ffi::RtpTransceiverInit {
    fn from(value: RtpTransceiverInit) -> Self {
        Self {
            direction: value.direction.into(),
            stream_ids: value.stream_ids,
            send_encodings: value.send_encodings.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone)]
pub struct RtpTransceiver {
    cxx_handle: SharedPtr<sys_rt::ffi::RtpTransceiver>,
}

impl Debug for RtpTransceiver {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("RtpTransceiver")
            .field("media_type", &self.media_type())
            .field("mid", &self.mid())
            .field("direction", &self.direction())
            .field("stopped", &self.stopped())
            .field("stopping", &self.stopping())
            .finish()
    }
}

impl RtpTransceiver {
    pub(crate) fn new(cxx_handle: SharedPtr<sys_rt::ffi::RtpTransceiver>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_rt::ffi::RtpTransceiver> {
        self.cxx_handle.clone()
    }

    pub fn media_type(&self) -> MediaType {
        self.cxx_handle.media_type().into()
    }

    pub fn mid(&self) -> Option<String> {
        self.cxx_handle.mid().ok()
    }

    pub fn sender(&self) -> RtpSender {
        RtpSender::new(self.cxx_handle.sender())
    }

    pub fn receiver(&self) -> RtpReceiver {
        RtpReceiver::new(self.cxx_handle.receiver())
    }

    pub fn stopped(&self) -> bool {
        self.cxx_handle.stopped()
    }

    pub fn stopping(&self) -> bool {
        self.cxx_handle.stopping()
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        self.cxx_handle.direction().into()
    }

    pub fn set_direction(&self, direction: RtpTransceiverDirection) -> Result<(), RTCError> {
        self.cxx_handle
            .set_direction(direction.into())
            .map_err(|e| unsafe { RTCError::from(e.what()) })
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        self.cxx_handle.current_direction().ok().map(Into::into)
    }

    pub fn fired_direction(&self) -> Option<RtpTransceiverDirection> {
        self.cxx_handle.fired_direction().ok().map(Into::into)
    }

    pub fn stop_standard(&self) -> Result<(), RTCError> {
        self.cxx_handle
            .stop_standard()
            .map_err(|e| unsafe { RTCError::from(e.what()) })
    }

    pub fn set_codec_preferences(&self, codecs: Vec<RtpCodecCapability>) -> Result<(), RTCError> {
        let ffi_codecs = codecs.into_iter().map(Into::into).collect();
        self.cxx_handle
            .set_codec_preferences(ffi_codecs)
            .map_err(|e| unsafe { RTCError::from(e.what()) })
    }

    pub fn codec_preferences(&self) -> Vec<RtpCodecCapability> {
        self.cxx_handle
            .codec_preferences()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn header_extensions_to_offer(&self) -> Vec<RtpHeaderExtensionCapability> {
        self.cxx_handle
            .header_extensions_to_offer()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn header_extensions_negotiated(&self) -> Vec<RtpHeaderExtensionCapability> {
        self.cxx_handle
            .header_extensions_negotiated()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn set_offered_rtp_header_extensions(
        &self,
        headers: Vec<RtpHeaderExtensionCapability>,
    ) -> Result<(), RTCError> {
        let ffi_headers = headers.into_iter().map(Into::into).collect();
        self.cxx_handle
            .set_offered_rtp_header_extensions(ffi_headers)
            .map_err(|e| unsafe { RTCError::from(e.what()) })
    }
}
