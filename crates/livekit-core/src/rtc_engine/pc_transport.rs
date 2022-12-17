use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tracing::{event, Level};

use livekit_webrtc::jsep::{IceCandidate, SessionDescription};
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnection, RTCOfferAnswerOptions, SignalingState,
};
use livekit_webrtc::rtc_error::RTCError;

use crate::proto::SignalTarget;

pub struct PCTransport {
    peer_connection: PeerConnection,
    signal_target: SignalTarget,
    pending_candidates: Vec<IceCandidate>,
    restarting_ice: bool,
}

impl Debug for PCTransport {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str("PCTransport")
    }
}

impl PCTransport {
    pub fn new(peer_connection: PeerConnection, signal_target: SignalTarget) -> Self {
        Self {
            signal_target,
            peer_connection,
            pending_candidates: Vec::default(),
            restarting_ice: false,
        }
    }

    pub fn prepare_ice_restart(&mut self) {
        self.restarting_ice = true;
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn add_ice_candidate(&mut self, ice_candidate: IceCandidate) -> Result<(), RTCError> {
        if self.peer_connection.remote_description().is_some() && !self.restarting_ice {
            self.peer_connection
                .add_ice_candidate(ice_candidate)
                .await?;
            return Ok(());
        }

        self.pending_candidates.push(ice_candidate);
        Ok(())
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn set_remote_description(
        &mut self,
        remote_description: SessionDescription,
    ) -> Result<(), RTCError> {
        self.peer_connection
            .set_remote_description(remote_description)
            .await?;

        for ic in self.pending_candidates.drain(..) {
            self.peer_connection.add_ice_candidate(ic).await?;
        }
        self.restarting_ice = false;
        Ok(())
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn create_anwser(
        &mut self,
        offer: SessionDescription,
        options: RTCOfferAnswerOptions,
    ) -> Result<SessionDescription, RTCError> {
        self.set_remote_description(offer).await?;
        let answer = self.peer_connection.create_answer(offer).await?;
        self.peer_connection
            .set_local_description(answer.clone())
            .await?;
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn create_offer(
        &mut self,
        options: RTCOfferAnswerOptions,
    ) -> Result<SessionDescription, RTCError> {
        if options.ice_restart {
            event!(Level::TRACE, "restarting ICE");
            self.restarting_ice = true;
            self.peer_connection().restart_ice();
        }

        let offer = self.peer_connection.create_offer(options).await?;
        self.peer_connection
            .set_local_description(offer.clone())
            .await?;
        Ok(offer)
    }
}

impl PCTransport {
    pub fn is_connected(&self) -> bool {
        self.peer_connection.ice_connection_state() == IceConnectionState::IceConnectionConnected
            || self.peer_connection.ice_connection_state()
                == IceConnectionState::IceConnectionCompleted
    }

    pub fn peer_connection(&mut self) -> &mut PeerConnection {
        &mut self.peer_connection
    }

    pub fn signal_target(&self) -> SignalTarget {
        self.signal_target.clone()
    }
}
