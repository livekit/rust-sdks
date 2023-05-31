use std::fmt::Debug;

use cxx::SharedPtr;
use webrtc_sys::frame_transformer::ffi::AdaptedNativeFrameTransformer;

use crate::{
    imp::rtp_receiver as imp_rr, media_stream::MediaStreamTrack, rtp_parameters::RtpParameters,
};

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) handle: imp_rr::RtpReceiver,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        self.handle.track()
    }

    pub fn parameters(&self) -> RtpParameters {
        self.handle.parameters()
    }

    pub fn set_depacketizer_to_decoder_frame_transformer(&self, transformer:  SharedPtr<AdaptedNativeFrameTransformer>) {
        println!("Called!");
        self.handle.set_depacketizer_to_decoder_frame_transformer(transformer);
    }

    pub fn new_adapted_frame_transformer(&self) -> SharedPtr<AdaptedNativeFrameTransformer> {
        self.handle.new_adapted_frame_transformer()
    }
}

impl Debug for RtpReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("track", &self.track())
            .field("cname", &self.parameters().rtcp.cname)
            .finish()
    }
}
