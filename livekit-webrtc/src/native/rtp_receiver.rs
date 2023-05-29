use super::media_stream::new_media_stream_track;
use crate::{media_stream::MediaStreamTrack, rtp_parameters::RtpParameters};
use cxx::SharedPtr;
use webrtc_sys::rtp_receiver as sys_rr;
use webrtc_sys::frame_transformer as sys_ft;

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) sys_handle: SharedPtr<sys_rr::ffi::RtpReceiver>,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        println!("RtpReceiver::track()");
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub fn parameters(&self) -> RtpParameters {
        println!("RtpReceiver::parameters()");
        self.sys_handle.get_parameters().into()
    }

    // frame_transformer: SharedPtr<FrameTransformer>
    pub fn set_depacketizer_to_decoder_frame_transformer(&self) {
        // self.sys_handle.set_depacketizer_to_decoder_frame_transformer();
    }

    pub fn new_frame_transformer(&self) {
        // self.sys_handle.set_depacketizer_to_decoder_frame_transformer();
        sys_ft::ffi::new_frame_transformer();
    }
}
