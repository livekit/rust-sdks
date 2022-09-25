use std::sync::Arc;
use std::time::Duration;

use log::{error, trace};

use livekit_webrtc::jsep::{IceCandidate, SessionDescription};
use livekit_webrtc::peer_connection::{PeerConnection, RTCOfferAnswerOptions, SdpError, SignalingState};
use livekit_webrtc::peer_connection_factory::RTCConfiguration;
use livekit_webrtc::rtc_error::RTCError;

use crate::lk_runtime::LKRuntime;

const NEGOTIATION_FREQUENCY: Duration = Duration::from_millis(150); // TODO(theomonnom)

pub type OnOfferHandler = Box<dyn FnMut(SessionDescription)>;

pub struct PCTransport {
    peer_connection: PeerConnection,
    pending_candidates: Vec<IceCandidate>,
    on_offer_handler: Option<OnOfferHandler>,
    restarting_ice: bool,
    renegotiate: bool,
}

impl PCTransport {
    pub fn new(lk_runtime: Arc<LKRuntime>, cfg: RTCConfiguration) -> Result<Self, RTCError> {
        let peer_connection = lk_runtime.pc_factory.create_peer_connection(cfg)?;

        Ok(Self {
            peer_connection,
            pending_candidates: Vec::default(),
            on_offer_handler: None,
            restarting_ice: false,
            renegotiate: false,
        })
    }

    pub fn peer_connection(&mut self) -> &mut PeerConnection {
        &mut self.peer_connection
    }

    pub fn on_offer(&mut self, handler: OnOfferHandler) {
        self.on_offer_handler = Some(handler);
    }

    pub fn add_ice_candidate(&mut self, ice_candidate: IceCandidate) {
        if self.peer_connection.remote_description().is_none() {
            self.pending_candidates.push(ice_candidate);
            return;
        }

        self.peer_connection.add_ice_candidate(ice_candidate);
    }

    pub async fn set_remote_description(&mut self, remote_description: SessionDescription) -> Result<(), SdpError> {
        self.peer_connection.set_remote_description(remote_description).await?;

        for ic in self.pending_candidates.drain(..) {
            self.peer_connection.add_ice_candidate(ic);
        }
        self.restarting_ice = false;

        if self.renegotiate {
            self.renegotiate = false;
            self.create_and_send_offer(RTCOfferAnswerOptions::default()).await?;
        }

        Ok(())
    }

    pub async fn negotiate(&mut self) -> Result<(), SdpError> {
        // TODO(theomonnom) Debounce here with NEGOTIATION_FREQUENCY
        self.create_and_send_offer(RTCOfferAnswerOptions::default()).await
    }

    async fn create_and_send_offer(&mut self, options: RTCOfferAnswerOptions) -> Result<(), SdpError> {
        if self.on_offer_handler.is_none() {
            return Ok(());
        }

        if options.ice_restart {
            trace!("restarting ICE");
            self.restarting_ice = true;
        }

        if self.peer_connection.signaling_state() == SignalingState::HaveLocalOffer {
            if options.ice_restart {
                if let Some(remote_description) = self.peer_connection.remote_description() {
                    self.peer_connection.set_remote_description(remote_description).await?;
                } else {
                    error!("trying to ice restart when the pc doesn't have remote description");
                }
            } else {
                self.renegotiate = true;
                return Ok(());
            }
        }

        let offer = self.peer_connection.create_offer(options).await?;
        trace!("created offer {:?}", offer);
        self.peer_connection.set_local_description(offer.clone()).await?;
        self.on_offer_handler.as_mut().unwrap()(offer);
        Ok(())
    }
}