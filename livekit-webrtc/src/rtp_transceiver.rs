use crate::imp::rtp_transceiver as imp_rt;
use crate::rtp_parameters::RtpEncodingParameters;
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_sender::RtpSender;
use crate::RtcError;

#[derive(Debug, Clone)]
pub struct RtpTransceiverInit {
    pub direction: RtpTransceiverDirection,
    pub stream_ids: Vec<String>,
    pub send_encodings: Vec<RtpEncodingParameters>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RtpTransceiverDirection {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
    Stopped,
}

#[derive(Clone)]
pub struct RtpTransceiver {
    pub(crate) handle: imp_rt::RtpTransceiver,
}

impl RtpTransceiver {
    pub fn mid(&self) -> Option<String> {
        self.handle.mid()
    }

    pub fn current_direction(&self) -> Option<RtpTransceiverDirection> {
        self.handle.current_direction()
    }

    pub fn direction(&self) -> RtpTransceiverDirection {
        self.handle.direction()
    }

    pub fn sender(&self) -> RtpSender {
        self.handle.sender()
    }

    pub fn receiver(&self) -> RtpReceiver {
        self.handle.receiver()
    }

    pub fn stop(&self) -> Result<(), RtcError> {
        self.handle.stop()
    }
}
