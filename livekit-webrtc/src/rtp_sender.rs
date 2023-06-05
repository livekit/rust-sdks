use std::fmt::Debug;

use crate::{
    imp::rtp_sender as imp_rs, media_stream_track::MediaStreamTrack, rtp_parameters::RtpParameters,
    RtcError,
};

#[derive(Clone)]
pub struct RtpSender {
    pub(crate) handle: imp_rs::RtpSender,
}

impl RtpSender {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        self.handle.track()
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        self.handle.set_track(track)
    }

    pub fn parameters(&self) -> RtpParameters {
        self.handle.parameters()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        self.handle.set_parameters(parameters)
    }
}

impl Debug for RtpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtpReceiver")
            .field("cname", &self.parameters().rtcp.cname)
            .finish()
    }
}
