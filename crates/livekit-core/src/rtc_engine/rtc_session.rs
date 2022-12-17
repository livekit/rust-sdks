use parking_lot::{Mutex, RwLock};
use prost_types::DurationError;
use std::ops::Residual;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::task::JoinHandle;

use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::time::sleep;

use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};

use crate::{proto, signal_client};
use livekit_webrtc::data_channel::{DataChannel, DataChannelInit, DataSendError, DataState};
use livekit_webrtc::jsep::{IceCandidate, SdpParseError, SessionDescription};
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnectionState, RTCOfferAnswerOptions, SignalingState,
};
use livekit_webrtc::peer_connection_factory::RTCConfiguration;

use crate::proto::data_packet::Value;
use crate::proto::{
    data_packet, signal_request, signal_response, DataPacket, JoinResponse, ParticipantUpdate,
    SignalTarget, TrickleRequest,
};
use crate::rtc_engine::lk_runtime::LKRuntime;
use crate::rtc_engine::pc_transport::PCTransport;
use crate::rtc_engine::rtc_events::{RTCEmitter, RTCEvent, RTCEvents};
use crate::signal_client::{SignalClient, SignalError, SignalEvent, SignalEvents, SignalOptions};

use super::{rtc_events, EngineEmitter, EngineError, EngineEvent, EngineEvents, EngineResult};

pub const LOSSY_DC_LABEL: &str = "_lossy";
pub const RELIABLE_DC_LABEL: &str = "_reliable";
pub const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// This struct holds a WebRTC session
/// The session changes at every reconnection
#[derive(Debug)]
pub struct RTCSession {
    join_response: JoinResponse,
    has_published: AtomicBool,

    publisher_pc: AsyncMutex<PCTransport>,
    subscriber_pc: AsyncMutex<PCTransport>,

    // Publisher data channels
    // Used to send data to other participants ( The SFU forwards the messages )
    lossy_dc: DataChannel,
    reliable_dc: DataChannel,

    // Subscriber data channels
    // These fields are never used, we just keep a strong reference to them,
    // so we can receive data from other participants
    sub_reliable_dc: Mutex<Option<DataChannel>>,
    sub_lossy_dc: Mutex<Option<DataChannel>>,

    renegotiate_publisher: AtomicBool,
    rtc_emitter: RTCEmitter,
}

impl RTCSession {
    pub fn configure(
        lk_runtime: Arc<LKRuntime>,
        join_response: JoinResponse,
    ) -> EngineResult<(Arc<Self>, RTCEvents)> {
        let (rtc_emitter, events) = mpsc::unbounded_channel();
        let rtc_config = RTCConfiguration::from(join_response.clone());

        let mut publisher_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
            SignalTarget::Publisher,
        );

        let mut subscriber_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
            SignalTarget::Subscriber,
        );

        let mut lossy_dc = publisher_pc.peer_connection().create_data_channel(
            LOSSY_DC_LABEL,
            DataChannelInit {
                ordered: true,
                max_retransmits: Some(0),
                ..DataChannelInit::default()
            },
        )?;

        let mut reliable_dc = publisher_pc.peer_connection().create_data_channel(
            RELIABLE_DC_LABEL,
            DataChannelInit {
                ordered: true,
                ..DataChannelInit::default()
            },
        )?;

        rtc_events::forward_pc_events(&mut publisher_pc, rtc_emitter.clone());
        rtc_events::forward_pc_events(&mut subscriber_pc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut lossy_dc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut reliable_dc, rtc_emitter.clone());

        Ok((
            Arc::new(Self {
                join_response,
                has_published: AtomicBool::new(false),
                publisher_pc: AsyncMutex::new(publisher_pc),
                subscriber_pc: AsyncMutex::new(subscriber_pc),
                lossy_dc,
                reliable_dc,
                sub_lossy_dc: Default::default(),
                sub_reliable_dc: Default::default(),
                renegotiate_publisher: AtomicBool::new(false),
                rtc_emitter,
            }),
            events,
        ))
    }

    pub fn use_data_channel(&self, data_channel: DataChannel, target: SignalTarget) {
        if target == SignalTarget::Subscriber {
            if data_channel.label() == RELIABLE_DC_LABEL {
                *self.sub_reliable_dc.lock() = Some(data_channel);
            } else {
                *self.sub_lossy_dc.lock() = Some(data_channel);
            }
        }
    }

    pub async fn use_ice_candidate(
        &self,
        ice_candidate: IceCandidate,
        target: SignalTarget,
    ) -> EngineResult<()> {
        if target == SignalTarget::Publisher {
            self.publisher_pc
                .lock()
                .await
                .add_ice_candidate(ice_candidate)
                .await?;
        } else {
            self.subscriber_pc
                .lock()
                .await
                .add_ice_candidate(ice_candidate)
                .await?;
        }
        Ok(())
    }

    pub async fn use_publisher_answer(&self, anwser: SessionDescription) -> EngineResult<()> {
        self.publisher_pc
            .lock()
            .await
            .set_remote_description(anwser)
            .await?;

        if self.renegotiate_publisher.compare_exchange(
            true,
            false,
            Ordering::Acquire,
            Ordering::Release,
        ) {
            self.request_publisher_negotiation().await;
        }

        Ok(())
    }

    pub async fn use_subscriber_offer(
        &self,
        offer: SessionDescription,
        options: RTCOfferAnswerOptions,
    ) -> EngineResult<SessionDescription> {
        Ok(self
            .subscriber_pc
            .lock()
            .await
            .create_anwser(offer, options)
            .await?)
    }

    pub async fn create_publisher_offer(
        &self,
        options: RTCOfferAnswerOptions,
    ) -> EngineResult<SessionDescription> {
        Ok(self.publisher_pc.lock().await.create_offer(options).await?)
    }

    pub async fn request_publisher_negotiation(&self) {
        self.has_published.store(true, Ordering::SeqCst);
        // Check if we are already waiting for the remote peer to accept our offer
        // If so, delay the renegotiation when the current one finished
        if self
            .publisher_pc
            .lock()
            .await
            .peer_connection()
            .signaling_state()
            == SignalingState::HaveLocalOffer
        {
            self.renegotiate_publisher.store(true, Ordering::SeqCst);
            return;
        }

        self.rtc_emitter.send(RTCEvent::NegotiationNeeded {
            target: SignalTarget::Publisher,
        })
    }

    pub async fn ensure_publisher_connected(&self, kind: data_packet::Kind) -> EngineResult<()> {
        if !self.join_response.subscriber_primary {
            return Ok(());
        }

        if !self.publisher_pc.lock().await.is_connected()
            && self
                .publisher_pc
                .lock()
                .await
                .peer_connection()
                .ice_connection_state()
                != IceConnectionState::IceConnectionChecking
        {
            let _ = self.request_publisher_negotiation().await;
        }

        let dc = self.data_channel(kind);
        if dc.state() == DataState::Open {
            return Ok(());
        }

        // Wait until the PeerConnection is connected
        let wait_connected = async {
            while self.publisher_pc.lock().await.is_connected() && dc.state() == DataState::Open {
                sleep(Duration::from_millis(50)).await;
            }
        };

        tokio::select! {
            _ = wait_connected => Ok(()),
            _ = sleep(MAX_ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("could not establish publisher connection: timeout".to_string());
                error!(error = ?err);
                Err(err)
            }
        }
    }
}

impl RTCSession {
    pub fn join_response(&self) -> &JoinResponse {
        &self.join_response
    }

    pub fn data_channel(&self, kind: data_packet::Kind) -> &DataChannel {
        if kind == data_packet::Kind::Reliable {
            &self.reliable_dc
        } else {
            &self.lossy_dc
        }
    }

    pub fn subscriber(&self) -> AsyncMutex<PCTransport> {
        self.subscriber_pc
    }

    pub fn publisher(&self) -> AsyncMutex<PCTransport> {
        self.publisher_pc
    }
}
