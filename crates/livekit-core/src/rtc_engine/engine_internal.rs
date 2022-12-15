use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::time::sleep;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};

use crate::{proto, signal_client};
use livekit_webrtc::data_channel::{DataChannel, DataChannelInit, DataSendError, DataState};
use livekit_webrtc::jsep::{IceCandidate, SdpParseError, SessionDescription};
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnectionState, RTCOfferAnswerOptions,
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
//
// TODO(theomonnom): Smarter retry intervals
pub(crate) const RECONNECT_ATTEMPTS: u32 = 10;
pub(crate) const RECONNECT_INTERVAL: Duration = Duration::from_millis(300);

pub(crate) const LOSSY_DC_LABEL: &str = "_lossy";
pub(crate) const RELIABLE_DC_LABEL: &str = "_reliable";
pub(crate) const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PCState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

#[derive(Debug, Default)]
pub struct SessionInfo {
    url: String,
    token: String,
    options: SignalOptions,
}

#[derive(Debug)]
pub struct EngineInternal {
    lk_runtime: Arc<LKRuntime>,
    info: Mutex<SessionInfo>,
    signal_client: Arc<SignalClient>,
    session: Arc<RwLock<Option<RTCSession>>>,
    reconnecting: AtomicBool,
    closed: AtomicBool,
    engine_emitter: EngineEmitter,
}

/// This struct holds a WebRTC session
/// The session changes at every reconnection
#[derive(Debug)]
pub struct RTCSession {
    join_response: Mutex<JoinResponse>,
    has_published: AtomicBool,
    pc_state: AtomicU8, // Casted to PCState enum

    publisher_pc: AsyncMutex<PCTransport>,
    subscriber_pc: AsyncMutex<PCTransport>,

    // Publisher data channels
    // Used to send data to other participants ( The SFU forwards the messages )
    lossy_dc: Mutex<DataChannel>,
    reliable_dc: Mutex<DataChannel>,

    // Subscriber data channels
    // These fields are never used, we just keep a strong reference to them,
    // so we can receive data from other participants
    sub_reliable_dc: Mutex<Option<DataChannel>>,
    sub_lossy_dc: Mutex<Option<DataChannel>>,
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct IceCandidateJSON {
    sdpMid: String,
    sdpMLineIndex: i32,
    candidate: String,
}

impl RTCSession {
    pub fn configure(
        lk_runtime: Arc<LKRuntime>,
        join_response: JoinResponse,
    ) -> EngineResult<(Self, RTCEvents)> {
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
            Self {
                join_response: Mutex::new(join_response),
                has_published: AtomicBool::new(false),
                pc_state: AtomicU8::new(PCState::New as u8),
                publisher_pc: AsyncMutex::new(publisher_pc),
                subscriber_pc: AsyncMutex::new(subscriber_pc),
                lossy_dc: Mutex::new(lossy_dc),
                reliable_dc: Mutex::new(reliable_dc),
                sub_lossy_dc: Mutex::new(None),
                sub_reliable_dc: Mutex::new(None),
            },
            events,
        ))
    }

    async fn negotiate_publisher(&self) -> EngineResult<()> {
        self.has_published.store(true, Ordering::SeqCst);
        let res = self.publisher_pc.lock().await.negotiate().await;
        if let Err(err) = &res {
            error!("failed to negotiate the publisher: {:?}", err);
        }
        res.map_err(Into::into)
    }

    async fn ensure_publisher_connected(&self, kind: data_packet::Kind) -> EngineResult<()> {
        if !self.join_response.lock().subscriber_primary {
            return Ok(());
        }

        let publisher = &self.publisher_pc;
        {
            let mut publisher = publisher.lock().await;
            if !publisher.is_connected()
                && publisher.peer_connection().ice_connection_state()
                    != IceConnectionState::IceConnectionChecking
            {
                let _ = self.negotiate_publisher().await;
            }
        }

        let dc = self.data_channel(kind);
        if dc.lock().state() == DataState::Open {
            return Ok(());
        }

        // Wait until the PeerConnection is connected
        let wait_connected = async move {
            while publisher.lock().await.is_connected() && dc.lock().state() == DataState::Open {
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

    async fn wait_pc_connection(&self) -> EngineResult<()> {
        let wait_connected = async move {
            while self.pc_state.load(Ordering::SeqCst) != PCState::Connected as u8 {
                sleep(Duration::from_millis(50)).await;
            }
        };

        tokio::select! {
            _ = wait_connected => Ok(()),
            _ = sleep(MAX_ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("wait_pc_connection timed out".to_string());
                Err(err)
            }
        }
    }

    fn data_channel(&self, kind: data_packet::Kind) -> &Mutex<DataChannel> {
        if kind == data_packet::Kind::Reliable {
            &self.reliable_dc
        } else {
            &self.lossy_dc
        }
    }
}

impl Default for EngineInternal {
    fn default() -> Self {
        let mut lk_runtime = None;
        {
            let mut lk_runtime_ref = LK_RUNTIME.lock();
            lk_runtime = lk_runtime_ref.upgrade();

            if lk_runtime.is_none() {
                let new_runtime = Arc::new(LKRuntime::default());
                *lk_runtime_ref = Arc::downgrade(&new_runtime);
                lk_runtime = Some(new_runtime);
            }
        }

        Self {
            lk_runtime: lk_runtime.unwrap(),
            ..Default::default()
        }
    }
}

impl EngineInternal {
    #[tracing::instrument]
    pub async fn connect(
        self: Arc<Self>,
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<EngineEvents> {
        let mut signal_events = self.signal_client.connect(url, token, options).await?;
        let join_response = signal_client::utils::next_join_response(&mut signal_events).await?;
        debug!("received JoinResponse: {:?}", join_response);

        let (session, rtc_events) =
            RTCSession::configure(self.lk_runtime.clone(), join_response.clone())?;
        let session = Arc::new(RwLock::new(Some(session)));

        let (engine_emitter, engine_events) = mpsc::channel(8);

        tokio::spawn(self.clone().signal_task(signal_events));
        tokio::spawn(self.clone().engine_task(rtc_events));

        if !join_response.subscriber_primary {
            session
                .read()
                .as_ref()
                .unwrap()
                .negotiate_publisher()
                .await?;
        }

        Ok(engine_events)
    }

    async fn engine_task(self: Arc<Self>, mut rtc_events: RTCEvents) {
        while let Some(event) = rtc_events.recv().await {
            if let Err(err) = self.handle_rtc(event).await {
                error!("failed to handle rtc event: {:?}", err);
            }
        }
    }

    async fn signal_task(self: Arc<Self>, mut signal_events: SignalEvents) {
        while let Some(signal) = signal_events.recv().await {
            match signal {
                SignalEvent::Open => {}
                SignalEvent::Signal(signal) => {
                    if let Err(err) = self.handle_signal(signal).await {
                        error!("failed to handle signal: {:?}", err);
                    }
                }
                SignalEvent::Close => {
                    self.handle_disconnected();
                }
            }
        }
    }

    async fn handle_rtc(self: &Arc<Self>, event: RTCEvent) -> EngineResult<()> {
        match event {
            RTCEvent::IceCandidate {
                ice_candidate,
                target,
            } => {
                let json = serde_json::to_string(&IceCandidateJSON {
                    sdpMid: ice_candidate.sdp_mid(),
                    sdpMLineIndex: ice_candidate.sdp_mline_index(),
                    candidate: ice_candidate.candidate(),
                })?;

                trace!("sending ice_candidate ({:?}) - {:?}", target, ice_candidate);

                tokio::spawn({
                    let signal_client = self.signal_client.clone();
                    async move {
                        signal_client
                            .send(signal_request::Message::Trickle(TrickleRequest {
                                candidate_init: json,
                                target: target as i32,
                            }))
                            .await;
                    }
                });
            }
            RTCEvent::ConnectionChange { state, target } => {
                trace!("connection change, {:?} {:?}", state, target);
                let subscriber_primary = self
                    .session
                    .read()
                    .as_ref()
                    .unwrap()
                    .join_response
                    .lock()
                    .subscriber_primary;

                let is_primary = subscriber_primary && target == SignalTarget::Subscriber;

                if is_primary && state == PeerConnectionState::Connected {
                    let old_state = self
                        .session
                        .read()
                        .as_ref()
                        .unwrap()
                        .pc_state
                        .swap(PCState::Connected as u8, Ordering::SeqCst);
                    if old_state == PCState::New as u8 {
                        let _ = self.engine_emitter.send(EngineEvent::Connected).await;
                        // First time connected
                    }
                } else if state == PeerConnectionState::Failed {
                    self.session
                        .read()
                        .as_ref()
                        .unwrap()
                        .pc_state
                        .store(PCState::Disconnected as u8, Ordering::SeqCst);

                    self.handle_disconnected();
                }
            }
            RTCEvent::DataChannel {
                data_channel,
                target,
            } => {
                if target == SignalTarget::Subscriber {
                    if data_channel.label() == RELIABLE_DC_LABEL {
                        *self.session.read().as_ref().unwrap().sub_reliable_dc.lock() =
                            Some(data_channel);
                    } else {
                        *self.session.read().as_ref().unwrap().sub_lossy_dc.lock() =
                            Some(data_channel);
                    }
                }
            }
            RTCEvent::Offer { offer, target } => {
                if target == SignalTarget::Publisher {
                    // Send the publisher offer to the server
                    tokio::spawn({
                        let signal_client = self.signal_client.clone();
                        async move {
                            signal_client
                                .send(signal_request::Message::Offer(proto::SessionDescription {
                                    r#type: "offer".to_string(),
                                    sdp: offer.to_string(),
                                }))
                                .await;
                        }
                    });
                }
            }
            RTCEvent::AddTrack {
                rtp_receiver,
                streams,
                target,
            } => {
                if target == SignalTarget::Subscriber {
                    let _ = self
                        .engine_emitter
                        .send(EngineEvent::AddTrack {
                            rtp_receiver,
                            streams,
                        })
                        .await;
                }
            }
            RTCEvent::Data { data, binary } => {
                if !binary {
                    Err(EngineError::Internal(
                        "text messages aren't supported".to_string(),
                    ))?;
                }

                let data = DataPacket::decode(&*data)?;
                match data.value.unwrap() {
                    Value::User(user) => {
                        // TODO(theomonnom) Send event
                    }
                    Value::Speaker(_) => {
                        // TODO(theomonnonm)
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_signal(self: &Arc<Self>, event: signal_response::Message) -> EngineResult<()> {
        match event {
            signal_response::Message::Answer(answer) => {
                trace!("received answer from the publisher: {:?}", answer);

                let sdp = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                self.session
                    .read()
                    .as_ref()
                    .unwrap()
                    .publisher_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
            }
            signal_response::Message::Offer(offer) => {
                // Handle the subscriber offer & send an answer to livekit-server
                // We always get an offer from the server when connecting
                trace!("received offer for the subscriber: {:?}", offer);
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;

                let session = self.session.read();
                let mut subscriber_pc = session.as_ref().unwrap().subscriber_pc.lock().await;

                subscriber_pc.set_remote_description(sdp).await?;
                let answer = subscriber_pc
                    .peer_connection()
                    .create_answer(RTCOfferAnswerOptions::default())
                    .await?;
                subscriber_pc
                    .peer_connection()
                    .set_local_description(answer.clone())
                    .await?;

                tokio::spawn({
                    let signal_client = self.signal_client.clone();
                    async move {
                        signal_client
                            .send(signal_request::Message::Answer(proto::SessionDescription {
                                r#type: "answer".to_string(),
                                sdp: answer.to_string(),
                            }))
                            .await;
                    }
                });
            }
            signal_response::Message::Trickle(trickle) => {
                // Add the IceCandidate received from the livekit-server
                let json: IceCandidateJSON = serde_json::from_str(&trickle.candidate_init)?;
                let ice = IceCandidate::from(&json.sdpMid, json.sdpMLineIndex, &json.candidate)?;

                trace!(
                    "received ice_candidate {:?} - {:?}",
                    SignalTarget::from_i32(trickle.target).unwrap(),
                    ice
                );

                if trickle.target == SignalTarget::Publisher as i32 {
                    self.session
                        .read()
                        .as_ref()
                        .unwrap()
                        .publisher_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                } else {
                    self.session
                        .read()
                        .as_ref()
                        .unwrap()
                        .subscriber_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                }
            }
            signal_response::Message::Update(update) => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::ParticipantUpdate(update))
                    .await;
            }
            _ => {}
        }

        Ok(())
    }
}

/// Reconnection Logic for the RTCEngine, it is responsable for: TODO
impl EngineInternal {
    async fn handle_disconnected(self: &Arc<Self>) {
        if self.closed.load(Ordering::SeqCst) || self.reconnecting.load(Ordering::SeqCst) {
            return;
        }

        self.reconnecting.store(true, Ordering::SeqCst);
        warn!("RTCEngine disconnected unexpectedly, reconnecting...");

        let mut full_reconnect = false;
        for i in 0..RECONNECT_ATTEMPTS {
            if full_reconnect {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Restarting).await;
                }

                info!("restarting connection... attempt: {}", i);
                if let Err(err) = self.try_restart_connection().await {
                    error!("restarting connection failed: {}", err);
                } else {
                    return;
                }
            } else {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Resuming).await;
                }

                info!("resuming connection... attempt: {}", i);
                if let Err(err) = self.try_resume_connection().await {
                    error!("resuming connection failed: {}", err);
                    if let EngineError::Signal(_) = err {
                        full_reconnect = true;
                    }
                } else {
                    return;
                }
            }

            tokio::time::sleep(RECONNECT_INTERVAL).await;
        }
        error!("failed to reconnect after {} attemps", RECONNECT_ATTEMPTS);
        self.reconnecting.store(false, Ordering::SeqCst);

        // TODO DISCONNECT
    }

    async fn try_restart_connection(self: &Arc<Self>) -> EngineResult<()> {
        Ok(())
    }

    async fn try_resume_connection(self: &Arc<Self>) -> EngineResult<()> {
        let mut info = self.info.lock();
        info.options.sid = self
            .session
            .read()
            .as_ref()
            .unwrap()
            .join_response
            .lock()
            .participant
            .as_ref()
            .unwrap()
            .sid
            .clone();

        self.signal_client.close().await;
        self.signal_client
            .connect(&info.url, &info.token.clone(), info.options.clone())
            .await?;

        self.engine_emitter.send(EngineEvent::SignalResumed).await;

        self.session
            .read()
            .as_ref()
            .unwrap()
            .subscriber_pc
            .lock()
            .await
            .prepare_ice_restart();

        if self
            .session
            .read()
            .as_ref()
            .unwrap()
            .has_published
            .load(Ordering::SeqCst)
        {
            self.session
                .read()
                .as_ref()
                .unwrap()
                .publisher_pc
                .lock()
                .await
                .create_and_send_offer(RTCOfferAnswerOptions {
                    ice_restart: true,
                    ..Default::default()
                })
                .await?;
        }
        self.session
            .read()
            .as_ref()
            .unwrap()
            .wait_pc_connection()
            .await?;

        self.signal_client.flush_queue().await;
        self.engine_emitter.send(EngineEvent::Resumed);
        Ok(())
    }
}
