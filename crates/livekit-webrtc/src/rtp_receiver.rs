use crate::media_stream::MediaStreamTrack;
use cxx::UniquePtr;
use libwebrtc_sys::rtp_receiver as sys_rec;
use std::fmt::{Debug, Formatter};

pub struct RtpReceiver {
    cxx_handle: UniquePtr<sys_rec::ffi::RtpReceiver>,
}

impl Debug for RtpReceiver {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("track", &self.track())
            .finish()
    }
}

impl RtpReceiver {
    pub(crate) fn new(cxx_handle: UniquePtr<sys_rec::ffi::RtpReceiver>) -> Self {
        Self { cxx_handle }
    }

    pub fn track(&self) -> MediaStreamTrack {
        MediaStreamTrack::new(self.cxx_handle.track())
    }
}
