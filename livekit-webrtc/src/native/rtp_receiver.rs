use crate::media_stream::{MediaStream, MediaStreamTrackHandle};
use crate::rtp_parameters::RtpParameters;
use cxx::SharedPtr;
use std::fmt::{Debug, Formatter};
use webrtc_sys::rtp_receiver as sys_rec;
use webrtc_sys::webrtc as sys_webrtc;

pub use sys_webrtc::ffi::MediaType;

#[derive(Clone)]
pub struct RtpReceiver {
    cxx_handle: SharedPtr<sys_rec::ffi::RtpReceiver>,
}

impl Debug for RtpReceiver {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("track", &self.track())
            .field("media_type", &self.media_type())
            .field("id", &self.id())
            .finish()
    }
}

impl RtpReceiver {
    pub(crate) fn new(cxx_handle: SharedPtr<sys_rec::ffi::RtpReceiver>) -> Self {
        Self { cxx_handle }
    }

    pub(crate) fn cxx_handle(&self) -> SharedPtr<sys_rec::ffi::RtpReceiver> {
        self.cxx_handle.clone()
    }

    pub fn track(&self) -> MediaStreamTrackHandle {
        MediaStreamTrackHandle::new(self.cxx_handle.track())
    }

    pub fn stream_ids(&self) -> Vec<String> {
        self.cxx_handle.stream_ids()
    }

    pub fn streams(&self) -> Vec<MediaStream> {
        let ptrs = self.cxx_handle.streams();
        let mut vec = Vec::with_capacity(ptrs.len());
        for stream in ptrs {
            vec.push(MediaStream::new(stream.ptr));
        }
        vec
    }

    pub fn media_type(&self) -> MediaType {
        self.cxx_handle.media_type()
    }

    pub fn id(&self) -> String {
        self.cxx_handle.id()
    }

    pub fn parameters(&self) -> RtpParameters {
        self.cxx_handle.get_parameters().into()
    }

    pub fn set_jitter_buffer_minimum_delay(&self, delay_seconds: Option<f64>) {
        self.cxx_handle
            .set_jitter_buffer_minimum_delay(delay_seconds.is_some(), delay_seconds.unwrap_or(0.0));
    }
}
