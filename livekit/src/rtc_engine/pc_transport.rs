use crate::proto;
use livekit_webrtc::prelude::*;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tracing::{event, Level};

const NEGOTIATION_FREQUENCY: Duration = Duration::from_millis(150);

pub type OnOfferHandler = Box<
    dyn (FnMut(SessionDescription) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

pub struct PCTransport {
    signal_target: proto::SignalTarget,
    peer_connection: PeerConnection,
    pending_candidates: Vec<IceCandidate>,
    on_offer_handler: Option<OnOfferHandler>,
    renegotiate: bool,
    restarting_ice: bool,
}

impl Debug for PCTransport {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str("PCTransport")
    }
}

impl PCTransport {
    pub fn new(peer_connection: PeerConnection, signal_target: proto::SignalTarget) -> Self {
        Self {
            signal_target,
            peer_connection,
            pending_candidates: Vec::default(),
            on_offer_handler: None,
            restarting_ice: false,
            renegotiate: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.peer_connection.ice_connection_state() == IceConnectionState::IceConnectionConnected
            || self.peer_connection.ice_connection_state()
                == IceConnectionState::IceConnectionCompleted
    }

    pub fn peer_connection(&mut self) -> &mut PeerConnection {
        &mut self.peer_connection
    }

    pub fn signal_target(&self) -> proto::SignalTarget {
        self.signal_target.clone()
    }

    pub fn on_offer(&mut self, handler: OnOfferHandler) {
        self.on_offer_handler = Some(handler);
    }

    pub fn prepare_ice_restart(&mut self) {
        self.restarting_ice = true;
    }

    pub fn close(&mut self) {
        self.peer_connection.close();
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

        if self.renegotiate {
            self.renegotiate = false;
            self.create_and_send_offer(RTCOfferAnswerOptions::default())
                .await?;
        }

        Ok(())
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn negotiate(&mut self) -> Result<(), RTCError> {
        // TODO(theomonnom) Debounce here with NEGOTIATION_FREQUENCY
        self.create_and_send_offer(RTCOfferAnswerOptions::default())
            .await
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn create_anwser(
        &mut self,
        offer: SessionDescription,
        options: RTCOfferAnswerOptions,
    ) -> Result<SessionDescription, RTCError> {
        self.set_remote_description(offer).await?;
        let answer = self
            .peer_connection()
            .create_answer(RTCOfferAnswerOptions::default())
            .await?;
        self.peer_connection()
            .set_local_description(answer.clone())
            .await?;

        Ok(answer)
    }

    #[tracing::instrument(level = Level::DEBUG)]
    pub async fn create_and_send_offer(
        &mut self,
        options: RTCOfferAnswerOptions,
    ) -> Result<(), RTCError> {
        if self.on_offer_handler.is_none() {
            return Ok(());
        }

        if options.ice_restart {
            event!(Level::TRACE, "restarting ICE");
            self.restarting_ice = true;
        }

        if self.peer_connection.signaling_state() == SignalingState::HaveLocalOffer {
            if options.ice_restart {
                if let Some(remote_description) = self.peer_connection.remote_description() {
                    self.peer_connection
                        .set_remote_description(remote_description)
                        .await?;
                } else {
                    event!(
                        Level::ERROR,
                        "trying to restart ICE when the pc doesn't have remote description"
                    );
                }
            } else {
                self.renegotiate = true;
                return Ok(());
            }
        }

        let offer = self.peer_connection.create_offer(options).await?;
        self.peer_connection
            .set_local_description(offer.clone())
            .await?;
        self.on_offer_handler.as_mut().unwrap()(offer).await;
        Ok(())
    }
}
