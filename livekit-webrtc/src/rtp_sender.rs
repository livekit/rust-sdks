use crate::{
    imp::rtp_sender as imp_rs, media_stream::MediaStreamTrack, rtp_parameters::RtpParameters,
    RtcError,
};

#[derive(Clone)]
pub struct RtpSender {
    handle: imp_rs::RtpSender,
}

impl RtpSender {
    pub fn track(&self) -> Option<Box<dyn MediaStreamTrack>> {
        self.handle.track()
    }

    pub fn set_track(&self, track: Option<Box<dyn MediaStreamTrack>>) -> Result<(), RtcError> {
        self.handle.set_track(track)
    }

    pub fn parameters(&self) -> RtpParameters {
        self.handle.parameters()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        self.handle.set_parameters(parameters)
    }
}
