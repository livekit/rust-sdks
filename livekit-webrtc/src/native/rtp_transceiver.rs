use crate::imp::rtp_receiver::RtpReceiver;
use crate::imp::rtp_sender::RtpSender;
use crate::rtp_receiver;
use crate::rtp_sender;
use crate::rtp_transceiver::RtpTransceiverDirection;
use crate::RtcError;
use cxx::SharedPtr;
use webrtc_sys::rtp_transceiver as sys_rt;
use webrtc_sys::webrtc as sys_webrtc;

impl From<sys_webrtc::ffi::RtpTransceiverDirection> for RtpTransceiverDirection {
    fn from(value: sys_webrtc::ffi::RtpTransceiverDirection) -> Self {
        match value {
            sys_webrtc::ffi::RtpTransceiverDirection::SendRecv => Self::SendRecv,
            sys_webrtc::ffi::RtpTransceiverDirection::SendOnly => Self::SendOnly,
            sys_webrtc::ffi::RtpTransceiverDirection::RecvOnly => Self::RecvOnly,
            sys_webrtc::ffi::RtpTransceiverDirection::Inactive => Self::Inactive,
        }
    }
}

#[derive(Clone)]
pub struct RtpTransceiver {
    pub(crate) sys_handle: SharedPtr<sys_rt::ffi::RtpTransceiver>,
}

impl RtpTransceiver {
    pub fn mid(&self) -> Option<String> {
        self.sys_handle.mid().ok()
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        self.sys_handle.current_direction().ok().map(Into::into)
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        self.sys_handle.direction().into()
    }

    pub fn sender(&self) -> rtp_sender::RtpSender {
        rtp_sender::RtpSender {
            handle: RtpSender {
                sys_handle: self.sys_handle.sender(),
            },
        }
    }

    pub fn receiver(&self) -> rtp_receiver::RtpReceiver {
        rtp_receiver::RtpReceiver {
            handle: RtpReceiver {
                sys_handle: self.sys_handle.receiver(),
            },
        }
    }

    pub fn stop(&self) -> Result<(), RtcError> {
        self.sys_handle.stop();
        Ok(())
    }
}
