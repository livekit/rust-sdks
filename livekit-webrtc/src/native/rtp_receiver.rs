use super::media_stream::new_media_stream_track;
use crate::{media_stream::MediaStreamTrack, rtp_parameters::RtpParameters};
use cxx::SharedPtr;
use webrtc_sys::rtp_receiver as sys_rr;

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) sys_handle: SharedPtr<sys_rr::ffi::RtpReceiver>,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub fn parameters(&self) -> RtpParameters {
        self.sys_handle.get_parameters().into()
    }
}
