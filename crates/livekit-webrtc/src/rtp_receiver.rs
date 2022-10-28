use crate::media_stream::MediaStreamTrack;
use cxx::UniquePtr;
use libwebrtc_sys::rtp_receiver as sys_rec;

pub struct RtpReceiver {
    cxx_handle: UniquePtr<sys_rec::ffi::RtpReceiver>,
}

impl RtpReceiver {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_rec::ffi::RtpReceiver>) -> Self {
        Self { cxx_handle }
    }

    pub fn track(&self) -> MediaStreamTrack {
        MediaStreamTrack::new(self.cxx_handle.track())
    }
}
