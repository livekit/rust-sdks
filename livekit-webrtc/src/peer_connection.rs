use crate::data_channel::{DataChannel, DataChannelInit};
use crate::ice_candidate::IceCandidate;
use crate::imp::peer_connection as imp_pc;
use crate::media_stream::MediaStreamTrack;
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_sender::RtpSender;
use crate::rtp_transceiver::{RtpTransceiver, RtpTransceiverInit};
use crate::session_description::SessionDescription;
use crate::{MediaType, RtcError};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PeerConnectionState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceConnectionState {
    IceConnectionNew,
    IceConnectionChecking,
    IceConnectionConnected,
    IceConnectionCompleted,
    IceConnectionFailed,
    IceConnectionDisconnected,
    IceConnectionClosed,
    IceConnectionMax,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum IceGatheringState {
    IceGatheringNew,
    IceGatheringGathering,
    IceGatheringComplete,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SignalingState {
    Stable,
    HaveLocalOffer,
    HaveLocalPrAnswer,
    HaveRemoteOffer,
    HaveRemotePrAnswer,
    Closed,
}

#[derive(Debug, Clone)]
pub struct OfferOptions {
    pub ice_restart: bool,
    pub offer_to_receive_audio: bool,
    pub offer_to_receive_video: bool,
}

#[derive(Debug, Clone)]
pub struct AnswerOptions {}

#[derive(Clone)]
pub struct PeerConnection {
    sys_handle: imp_pc::PeerConnection,
}

impl PeerConnection {
    pub async fn create_offer(
        &self,
        options: OfferOptions,
    ) -> Result<SessionDescription, RtcError> {
        self.handle.create_offer(options).await
    }

    pub async fn create_answer(
        &self,
        options: AnswerOptions,
    ) -> Result<SessionDescription, RtcError> {
        self.handle.create_answer(options).await
    }

    pub async fn set_local_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        self.handle.set_local_description(desc).await
    }

    pub async fn set_remote_description(&self, desc: SessionDescription) -> Result<(), RtcError> {
        self.handle.set_remote_description(desc).await
    }

    pub async fn add_ice_candidate(&self, candidate: IceCandidate) -> Result<(), RtcError> {
        self.handle.add_ice_candidate(candidate).await
    }

    pub fn create_data_channel(
        &self,
        label: &str,
        init: DataChannelInit,
    ) -> Result<DataChannel, RtcError> {
        self.handle.create_data_channel(label, init)
    }

    pub fn add_track<T: AsRef<str>>(
        &self,
        track: Box<dyn MediaStreamTrack>,
        streams_ids: &[T],
    ) -> Result<RtpSender, RtcError> {
        self.handle.add_track(track, streams_ids)
    }

    pub fn remove_track(&self, sender: RtpSender) -> Result<(), RtcError> {
        self.handle.remove_track(sender)
    }

    pub fn add_transceiver(
        &self,
        track: Box<dyn MediaStreamTrack>,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        self.handle.add_transceiver(track, init)
    }

    pub fn add_transceiver_for_media(
        &self,
        media_type: MediaType,
        init: RtpTransceiverInit,
    ) -> Result<RtpTransceiver, RtcError> {
        self.handle.add_transceiver_for_media(media_type, init)
    }
    pub fn close(&self) {
        self.handle.close()
    }

    pub fn connection_state(&self) -> PeerConnectionState {
        self.handle.connection_state()
    }

    pub fn ice_connection_state(&self) -> IceConnectionState {
        self.handle.ice_connection_state()
    }

    pub fn ice_gathering_state(&self) -> IceGatheringState {
        self.handle.ice_gathering_state()
    }

    pub fn signaling_state(&self) -> SignalingState {
        self.handle.signaling_state()
    }

    pub fn current_local_description(&self) -> Option<SessionDescription> {
        self.handle.current_local_description()
    }

    pub fn current_remote_description(&self) -> Option<SessionDescription> {
        self.handle.current_remote_description()
    }

    pub fn senders(&self) -> Vec<RtpSender> {
        self.handle.senders()
    }

    pub fn receivers(&self) -> Vec<RtpReceiver> {
        self.handle.receivers()
    }

    pub fn transceivers(&self) -> Vec<RtpTransceiver> {
        self.handle.transceivers()
    }
}
