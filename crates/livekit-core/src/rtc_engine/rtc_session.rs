use livekit_webrtc::media_stream::{MediaStream, MediaStreamTrackHandle};
use livekit_webrtc::rtp_receiver::RtpReceiver;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::task::JoinHandle;

use tokio::sync::{broadcast, mpsc, watch, Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tokio::time::sleep;

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

pub const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const LOSSY_DC_LABEL: &str = "_lossy";
pub const RELIABLE_DC_LABEL: &str = "_reliable";

pub type SessionEmitter = mpsc::UnboundedSender<SessionEvent>;
pub type SessionEvents = mpsc::UnboundedReceiver<SessionEvent>;

#[derive(Debug)]
pub enum SessionEvent {
    Data {
        data: Vec<u8>,
    },
    MediaTrack {
        track: MediaStreamTrackHandle,
        stream: MediaStream,
        receiver: RtpReceiver,
    },
    Close {
        reason: String,
    },
    Connected,
}

#[repr(u8)]
pub enum PCState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct IceCandidateJSON {
    sdpMid: String,
    sdpMLineIndex: i32,
    candidate: String,
}

#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub url: String,
    pub token: String,
    pub options: SignalOptions,
    pub join_response: JoinResponse,
}

/// Fields shared with rtc_task and signal_task
#[derive(Debug)]
struct SessionInner {
    info: SessionInfo,
    signal_client: Arc<SignalClient>,
    pc_state: AtomicU8, // PCState
    has_published: AtomicBool,

    publisher_pc: AsyncMutex<PCTransport>,
    subscriber_pc: AsyncMutex<PCTransport>,

    // Publisher data channels
    // used to send data to other participants ( The SFU forwards the messages )
    lossy_dc: DataChannel,
    reliable_dc: DataChannel,

    // Keep a strong reference to the subscriber datachannels,
    // so we can receive data from other participants
    subscriber_dc: Mutex<Vec<DataChannel>>,

    emitter: SessionEmitter,
}

/// This struct holds a WebRTC session
/// The session changes at every reconnection
///
/// RTCSession is also responsable for the signaling and the negotation
#[derive(Debug)]
pub struct RTCSession {
    lk_runtime: Arc<LKRuntime>,
    inner: Arc<SessionInner>,
    close_emitter: watch::Sender<bool>, // false = is_running
    signal_task: JoinHandle<()>,
    rtc_task: JoinHandle<()>,
}

impl RTCSession {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
        lk_runtime: Arc<LKRuntime>,
        session_emitter: SessionEmitter,
    ) -> EngineResult<Self> {
        // Connect to the SignalClient
        let (signal_client, mut signal_events) = SignalClient::new();
        let signal_client = Arc::new(signal_client);
        signal_client.connect(url, token, options.clone()).await?;
        let join_response = signal_client::utils::next_join_response(&mut signal_events).await?;
        debug!("received JoinResponse: {:?}", join_response);

        let (rtc_emitter, rtc_events) = mpsc::unbounded_channel();
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

        // Forward events received in the Signaling Thread to our rtc channel
        rtc_events::forward_pc_events(&mut publisher_pc, rtc_emitter.clone());
        rtc_events::forward_pc_events(&mut subscriber_pc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut lossy_dc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut reliable_dc, rtc_emitter.clone());

        let session_info = SessionInfo {
            url: url.to_owned(),
            token: token.to_owned(),
            options,
            join_response,
        };

        // Start tasks
        let (close_emitter, close_receiver) = watch::channel(false);

        let inner = Arc::new(SessionInner {
            info: session_info,
            pc_state: AtomicU8::new(PCState::New as u8),
            has_published: Default::default(),
            signal_client,
            publisher_pc: AsyncMutex::new(publisher_pc),
            subscriber_pc: AsyncMutex::new(subscriber_pc),
            lossy_dc,
            reliable_dc,
            subscriber_dc: Default::default(),
            emitter: session_emitter,
        });

        // Start session tasks
        let signal_task = tokio::spawn(
            inner
                .clone()
                .signal_task(signal_events, close_receiver.clone()),
        );
        let rtc_task = tokio::spawn(inner.clone().rtc_task(rtc_events, close_receiver.clone()));

        let session = Self {
            lk_runtime,
            inner: inner.clone(),
            close_emitter,
            signal_task,
            rtc_task,
        };

        if !inner.info.join_response.subscriber_primary {
            inner.negotiate_publisher().await?;
        }

        Ok(session)
    }

    /// Close the PeerConnections and the SignalClient
    #[tracing::instrument]
    pub async fn close(self) {
        // Close the tasks
        self.close_emitter.send(true);
        self.rtc_task.await;
        self.signal_task.await;
        self.inner.close().await;
    }

    pub async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.inner.publish_data(data, kind).await
    }

    pub async fn wait_pc_connectiom(&self) -> EngineResult<()> {
        self.inner.wait_pc_connection().await
    }
}

impl RTCSession {
    pub fn info(&self) -> &SessionInfo {
        &self.inner.info
    }

    pub fn publisher(&self) -> &AsyncMutex<PCTransport> {
        &self.inner.publisher_pc
    }

    pub fn subscriber(&self) -> &AsyncMutex<PCTransport> {
        &self.inner.subscriber_pc
    }

    pub fn signal_client(&self) -> &Arc<SignalClient> {
        &self.inner.signal_client
    }

    pub fn data_channel(&self, kind: data_packet::Kind) -> &DataChannel {
        &self.inner.data_channel(kind)
    }
}

impl SessionInner {
    async fn rtc_task(
        self: Arc<Self>,
        mut rtc_events: RTCEvents,
        mut close_receiver: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                res = rtc_events.recv() => {
                    if let Some(event) = res {
                        if let Err(err) = self.on_rtc_event(event).await {
                            error!("failed to handle rtc event: {:?}", err);
                        }
                    } else {
                        panic!("rtc_events has been closed unexpectedly");
                    }
                },
                 _ = close_receiver.changed() => {
                    break;
                }
            }
        }
    }

    async fn signal_task(
        self: Arc<Self>,
        mut signal_events: SignalEvents,
        mut close_receiver: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                res = signal_events.recv() => {
                    if let Some(signal) = res {
                        match signal {
                            SignalEvent::Open => {}
                            SignalEvent::Signal(signal) => {
                                if let Err(err) = self.on_signal_event(signal).await {
                                    error!("failed to handle signal: {:?}", err);
                                }
                            }
                            SignalEvent::Close => {
                                self.on_session_disconnected("SignalClient closed");
                            }
                        }
                    } else {
                        panic!("signal_events has been closed unexpectedly");
                    }

                },
                _ = close_receiver.changed() => {
                    break;
                }
            }
        }
    }

    async fn on_signal_event(&self, event: signal_response::Message) -> EngineResult<()> {
        match event {
            signal_response::Message::Answer(answer) => {
                trace!("received publisher answer: {:?}", answer);
                let answer = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                self.publisher_pc
                    .lock()
                    .await
                    .set_remote_description(answer)
                    .await?;
            }
            signal_response::Message::Offer(offer) => {
                trace!("received subscriber offer: {:?}", offer);
                let offer = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;
                let answer = self
                    .subscriber_pc
                    .lock()
                    .await
                    .create_anwser(offer, RTCOfferAnswerOptions::default())
                    .await?;

                self.signal_client
                    .send(signal_request::Message::Answer(proto::SessionDescription {
                        r#type: "answer".to_string(),
                        sdp: answer.to_string(),
                    }))
                    .await;
            }
            signal_response::Message::Trickle(trickle) => {
                let target = SignalTarget::from_i32(trickle.target).unwrap();
                let ice_candidate = {
                    let json = serde_json::from_str::<IceCandidateJSON>(&trickle.candidate_init)?;
                    IceCandidate::from(&json.sdpMid, json.sdpMLineIndex, &json.candidate)?
                };

                trace!("received ice_candidate {:?} {:?}", target, ice_candidate);

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
            }
            _ => {}
        }

        Ok(())
    }

    async fn on_rtc_event(&self, event: RTCEvent) -> EngineResult<()> {
        match event {
            RTCEvent::IceCandidate {
                ice_candidate,
                target,
            } => {
                self.signal_client
                    .send(signal_request::Message::Trickle(TrickleRequest {
                        candidate_init: serde_json::to_string(&IceCandidateJSON {
                            sdpMid: ice_candidate.sdp_mid(),
                            sdpMLineIndex: ice_candidate.sdp_mline_index(),
                            candidate: ice_candidate.candidate(),
                        })?,
                        target: target as i32,
                    }))
                    .await;
            }
            RTCEvent::ConnectionChange { state, target } => {
                trace!("connection change, {:?} {:?}", state, target);
                let is_primary = self.info.join_response.subscriber_primary
                    && target == SignalTarget::Subscriber;

                if is_primary && state == PeerConnectionState::Connected {
                    let old_state = self
                        .pc_state
                        .swap(PCState::Connected as u8, Ordering::SeqCst);
                    if old_state == PCState::New as u8 {
                        let _ = self.emitter.send(SessionEvent::Connected);
                    }
                } else if state == PeerConnectionState::Failed {
                    self.pc_state
                        .store(PCState::Disconnected as u8, Ordering::SeqCst);

                    self.on_session_disconnected("pc_state failed");
                }
            }
            RTCEvent::DataChannel { data_channel } => {
                self.subscriber_dc.lock().push(data_channel);
            }
            RTCEvent::Offer { offer } => {
                // Send the publisher offer to the server
                self.signal_client
                    .send(signal_request::Message::Offer(proto::SessionDescription {
                        r#type: "offer".to_string(),
                        sdp: offer.to_string(),
                    }))
                    .await;
            }
            RTCEvent::AddTrack {
                rtp_receiver,
                mut streams,
            } => {
                if !streams.is_empty() {
                    let _ = self.emitter.send(SessionEvent::MediaTrack {
                        track: rtp_receiver.track(),
                        stream: streams.remove(0),
                        receiver: rtp_receiver,
                    });
                } else {
                    warn!("AddTrack event with no streams");
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

    /// Called when the SignalClient or one of the PeerConnection has lost the connection
    /// The RTCEngine may try a reconnect.
    fn on_session_disconnected(&self, reason: &str) {
        let _ = self.emitter.send(SessionEvent::Close {
            reason: reason.to_owned(),
        });
    }

    #[tracing::instrument]
    async fn close(&self) {
        self.signal_client.close().await;
        self.publisher_pc.lock().await.close();
        self.subscriber_pc.lock().await.close();
    }

    #[tracing::instrument(skip(data))]
    async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.ensure_publisher_connected(kind).await?;
        self.data_channel(kind)
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    /// Try to restart the session by doing an ICE Restart (The SignalClient is also restarted)
    /// This reconnection if more seemless than the full reconnection implemented in ['RTCEngine']
    async fn restart_session(&self) -> EngineResult<()> {
        self.signal_client.close().await;

        let mut options = self.info.options.clone();
        options.sid = self.info.join_response.participant.clone().unwrap().sid;
        options.reconnect = true;

        self.signal_client
            .connect(&self.info.url, &self.info.token, options)
            .await?;

        self.subscriber_pc.lock().await.prepare_ice_restart();

        if self.has_published.load(Ordering::Acquire) {
            self.publisher_pc
                .lock()
                .await
                .create_and_send_offer(RTCOfferAnswerOptions {
                    ice_restart: true,
                    ..Default::default()
                })
                .await?;
        }

        self.wait_pc_connection().await?;
        self.signal_client.flush_queue().await;

        Ok(())
    }

    // Wait for PCState to become PCState::Connected
    // Timeout after ['MAX_ICE_CONNECT_TIMEOUT']
    async fn wait_pc_connection(&self) -> EngineResult<()> {
        let wait_connected = async move {
            while self.pc_state.load(Ordering::Acquire) != PCState::Connected as u8 {
                tokio::task::yield_now().await;
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

    /// Start publisher negotiation
    async fn negotiate_publisher(&self) -> EngineResult<()> {
        self.has_published.store(true, Ordering::Release);
        let res = self.publisher_pc.lock().await.negotiate().await;
        if let Err(err) = &res {
            error!("failed to negotiate the publisher: {:?}", err);
        }
        res.map_err(Into::into)
    }

    /// Ensure the Publisher PC is connected, if not, start the negotiation
    /// This is required when sending data to the server
    async fn ensure_publisher_connected(&self, kind: data_packet::Kind) -> EngineResult<()> {
        if !self.info.join_response.subscriber_primary {
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
            let _ = self.negotiate_publisher().await;
        }

        let dc = self.data_channel(kind);
        if dc.state() == DataState::Open {
            return Ok(());
        }

        // Wait until the PeerConnection is connected
        let wait_connected = async {
            while self.publisher_pc.lock().await.is_connected() && dc.state() == DataState::Open {
                tokio::task::yield_now().await;
            }
        };

        // TODO(theomonnom) Avoid 15 seconds deadlock on the RTCEngine by recv close here
        tokio::select! {
            _ = wait_connected => Ok(()),
            _ = sleep(MAX_ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("could not establish publisher connection: timeout".to_string());
                error!(error = ?err);
                Err(err)
            }
        }
    }

    fn data_channel(&self, kind: data_packet::Kind) -> &DataChannel {
        if kind == data_packet::Kind::Reliable {
            &self.reliable_dc
        } else {
            &self.lossy_dc
        }
    }
}
