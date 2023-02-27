use crate::{
    imp::rtp_receiver as imp_rr, media_stream::MediaStreamTrack, rtp_parameters::RtpParameters,
};

#[derive(Clone)]
pub struct RtpReceiver {
    pub(crate) handle: imp_rr::RtpReceiver,
}

impl RtpReceiver {
    pub fn track(&self) -> Option<Box<dyn MediaStreamTrack>> {
        self.handle.track()
    }

    pub fn parameters(&self) -> RtpParameters {
        self.handle.parameters()
    }
}
