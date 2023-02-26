pub mod data_channel;
pub mod ice_candidate;
pub mod media_stream;
pub mod peer_connection;
pub mod peer_connection_factory;
pub mod rtp_parameters;
pub mod rtp_receiver;
pub mod rtp_sender;
pub mod rtp_transceiver;
pub mod session_description;
pub mod video_frame;
pub mod yuv_helper;

use crate::{RtcError, RtcErrorType};
use webrtc_sys::rtc_error as sys_err;

impl From<sys_err::ffi::RTCErrorType> for RtcErrorType {
    fn from(value: sys_err::ffi::RTCErrorType) -> Self {
        match value {
            sys_err::ffi::RTCErrorType::InvalidState => Self::InvalidState,
            _ => Self::Internal,
        }
    }
}

impl From<sys_err::ffi::RTCError> for RtcError {
    fn from(value: sys_err::ffi::RTCError) -> Self {
        Self {
            error_type: value.error_type.into(),
            message: value.message,
        }
    }
}
