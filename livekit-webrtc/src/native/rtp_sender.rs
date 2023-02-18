use crate::media_stream::MediaStreamTrackInternal;
use crate::prelude::*;
use crate::rtp_parameters::{RtpEncodingParameters, RtpParameters};
use cxx::SharedPtr;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use webrtc_sys::rtp_sender as sys_rs;
use webrtc_sys::webrtc as sys_webrtc;

pub use sys_webrtc::ffi::MediaType;

#[derive(Clone)]
pub struct RtpSender {
    cxx_handle: SharedPtr<sys_rs::ffi::RtpSender>,
}

impl Debug for RtpSender {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("RtpSender")
            .field("media_type", &self.media_type())
            .field("ssrc", &self.ssrc())
            .field("id", &self.id())
            .finish()
    }
}

impl RtpSender {
    pub(crate) fn new(cxx_handle: SharedPtr<sys_rs::ffi::RtpSender>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_rs::ffi::RtpSender> {
        self.cxx_handle.clone()
    }

    pub fn set_track(&self, track: Arc<dyn MediaStreamTrackInternal>) -> bool {
        self.cxx_handle.set_track(track.cxx_handle())
    }

    pub fn track(&self) -> Arc<dyn MediaStreamTrack> {
        crate::media_stream::new_track(self.cxx_handle.track())
    }

    pub fn ssrc(&self) -> u32 {
        self.cxx_handle.ssrc()
    }

    pub fn media_type(&self) -> MediaType {
        self.cxx_handle.media_type()
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }

    pub fn stream_ids(&self) -> Vec<String> {
        self.cxx_handle.stream_ids()
    }

    pub fn set_streams(&self, stream_ids: &Vec<String>) {
        self.cxx_handle.set_streams(stream_ids);
    }

    pub fn init_send_encodings(&self) -> Vec<RtpEncodingParameters> {
        self.cxx_handle
            .init_send_encodings()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn parameters(&self) -> RtpParameters {
        self.cxx_handle.get_parameters().into()
    }

    pub fn set_parameters(&self, params: RtpParameters) -> Result<(), RTCError> {
        self.cxx_handle
            .set_parameters(params.into())
            .map_err(|e| unsafe { RTCError::from(e.what()) })
    }
}
