use super::media_stream::new_media_stream_track;
use crate::{
    media_stream::MediaStreamTrack, rtp_parameters::RtpParameters, RtcError, RtcErrorType,
};
use cxx::SharedPtr;
use webrtc_sys::{rtc_error::ffi::RtcError, rtp_sender as sys_rs};

#[derive(Clone)]
pub struct RtpSender {
    pub(crate) sys_handle: SharedPtr<sys_rs::ffi::RtpSender>,
}

impl RtpSender {
    pub fn track(&self) -> Option<MediaStreamTrack> {
        let track_handle = self.sys_handle.track();
        if track_handle.is_null() {
            return None;
        }

        Some(new_media_stream_track(track_handle))
    }

    pub fn set_track(&self, track: Option<MediaStreamTrack>) -> Result<(), RtcError> {
        if !self
            .sys_handle
            .set_track(track.map_or(SharedPtr::null(), |t| t.sys_handle()))
        {
            return Err(RtcError {
                error_type: RtcErrorType::InvalidState,
                message: "Failed to set track".to_string(),
            });
        }

        Ok(())
    }

    pub fn parameters(&self) -> RtpParameters {
        self.sys_handle.get_parameters().into()
    }

    pub fn set_parameters(&self, parameters: RtpParameters) -> Result<(), RtcError> {
        self.sys_handle
            .set_parameters(parameters.into())
            .map_err(|e| unsafe { RtcError::from(e.what()).into() })
    }
}
