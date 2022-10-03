use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::Duration;

use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::{event, Level};

use livekit_webrtc::data_channel::{DataChannel, DataChannelInit, DataSendError, DataState};
use livekit_webrtc::jsep::{IceCandidate, SdpParseError, SessionDescription};
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnectionState, RTCOfferAnswerOptions,
};
use livekit_webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};
use livekit_webrtc::rtc_error::RTCError;

use crate::{proto, signal_client};
use crate::lk_runtime::LKRuntime;
use crate::pc_transport::PCTransport;
use crate::proto::{
    data_packet, DataPacket, JoinResponse, signal_request, signal_response, SignalTarget,
    TrickleRequest, UserPacket,
};
use crate::proto::data_packet::Value;
use crate::signal_client::{SignalClient, SignalError};

const LOSSY_DC_LABEL: &str = "_lossy";
const RELIABLE_DC_LABEL: &str = "_reliable";
const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct IceCandidateJSON {
    sdpMid: String,
    sdpMLineIndex: i32,
    candidate: String,
}

pub struct Packet {
    pub data: UserPacket,
    pub kind: data_packet::Kind,
}

pub type OnDataHandler =
Box<dyn (FnMut(Packet) -> Pin<Box<dyn Future<Output=()> + Send + 'static>>) + Send + Sync>;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("signal failure")]
    Signal(#[from] SignalError),
    #[error("internal webrtc failure")]
    Rtc(#[from] RTCError),
    #[error("failed to parse sdp")]
    Parse(#[from] SdpParseError),
    #[error("serde error")]
    Serde(#[from] serde_json::Error),
    #[error("failed to send data to the datachannel")]
    Data(#[from] DataSendError),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("decode error")]
    Decode(#[from] prost::DecodeError),
    #[error("internal error: {0}")]
    Internal(String), // Unexpected error
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PCState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

#[derive(Debug)]
pub enum EngineMessage {
    IceCandidate {
        ice_candidate: IceCandidate,
        publisher: bool,
    },
    ConnectionChange {
        state: PeerConnectionState,
        primary: bool,
    },
    PrimaryDataChannel {
        data_channel: DataChannel,
    },
    PublisherOffer {
        offer: SessionDescription,
    },
    Data {
        data: Vec<u8>,
        binary: bool,
    },
}


#[derive(Debug)]
pub struct RTCEngine {
    signal_client: Arc<SignalClient>,
    internal: Arc<EngineInternal>,

    #[allow(unused)]
    lk_runtime: Arc<LKRuntime>, // Keep a reference while we're using the RTCEngine
}

#[tracing::instrument(skip(url, token))]
pub async fn connect(url: &str, token: &str) -> Result<RTCEngine, EngineError> {
    // Acquire an existing/a new LKRuntime
    let mut lk_runtime_ref = LK_RUNTIME.lock().await;
    let mut lk_runtime = lk_runtime_ref.upgrade();

    if lk_runtime.is_none() {
        let new_runtime = Arc::new(LKRuntime::new());
        *lk_runtime_ref = Arc::downgrade(&new_runtime);
        lk_runtime = Some(new_runtime);
    }
    let lk_runtime = lk_runtime.unwrap();
    let signal_client = Arc::new(signal_client::connect(url, token).await?);

    if let Some(signal_response::Message::Join(join_response)) = signal_client.recv().await {
        event!(Level::DEBUG, "received JoinResponse: {:?}", join_response);
        let (sender, receiver) = mpsc::channel(8);
        let internal = Arc::new(EngineInternal::new(
            lk_runtime.clone(),
            sender,
            join_response.clone(),
        )?);

        if !join_response.subscriber_primary {
            internal.publisher_pc.lock().await.negotiate().await?;
        }

        tokio::spawn({
            let signal_client = signal_client.clone();
            let internal = internal.clone();

            async move {
                internal.run(receiver, signal_client).await;
            }
        });

        Ok(RTCEngine {
            lk_runtime,
            signal_client,
            internal,
        })
    } else {
        panic!("the first received message isn't a JoinResponse");
    }
}

impl RTCEngine {
    /// Send data to other participants in the Room
    #[tracing::instrument]
    pub async fn publish_data(
        &mut self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.internal.ensure_publisher_connected(kind).await?;
        self.internal.data_channel(kind)
            .lock()
            .await
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    /// Return the last received JoinResponse
    pub async fn join_response(&self) -> JoinResponse {
        self.internal.join_response.lock().await.clone()
    }

    pub async fn on_data(&self, f: OnDataHandler) {
        *self.internal.on_data_handler.lock().await = Some(f);
    }
}

struct EngineInternal {
    publisher_pc: Arc<Mutex<PCTransport>>,
    subscriber_pc: Arc<Mutex<PCTransport>>,
    lossy_dc: Arc<Mutex<DataChannel>>,
    reliable_dc: Arc<Mutex<DataChannel>>,
    lossy_dc_sub: Arc<Mutex<Option<DataChannel>>>,
    reliable_dc_sub: Arc<Mutex<Option<DataChannel>>>,

    msg_sender: mpsc::Sender<EngineMessage>,
    join_response: Mutex<JoinResponse>,
    pc_state: AtomicU8,
    // PCState
    has_published: AtomicBool,

    // Listeners
    on_data_handler: Arc<Mutex<Option<OnDataHandler>>>,
}

impl EngineInternal {
    /// New internal is created on connect & on reconnect
    /// It creates the PeerConnections, the DataChannels and the libwebrtc listeners
    #[tracing::instrument]
    fn new(
        lk_runtime: Arc<LKRuntime>,
        sender: mpsc::Sender<EngineMessage>,
        join: JoinResponse,
    ) -> Result<Self, EngineError> {
        let rtc_config = RTCConfiguration {
            ice_servers: {
                let mut servers = vec![];
                for is in join.ice_servers.clone() {
                    servers.push(ICEServer {
                        urls: is.urls,
                        username: is.username,
                        password: is.credential,
                    })
                }
                servers
            },
            continual_gathering_policy: ContinualGatheringPolicy::GatherContinually,
            ice_transport_type: IceTransportsType::All,
        };

        let mut publisher_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
        );
        let mut subscriber_pc =
            PCTransport::new(lk_runtime.pc_factory.create_peer_connection(rtc_config)?);

        publisher_pc.peer_connection().on_ice_candidate(Box::new({
            let sender = sender.clone();
            move |ice_candidate| {
                let _ = sender.blocking_send(EngineMessage::IceCandidate {
                    ice_candidate,
                    publisher: true,
                });
            }
        }));

        subscriber_pc.peer_connection().on_ice_candidate(Box::new({
            let sender = sender.clone();
            move |ice_candidate| {
                let _ = sender.blocking_send(EngineMessage::IceCandidate {
                    ice_candidate,
                    publisher: false,
                });
            }
        }));

        publisher_pc.on_offer({
            let sender = sender.clone();
            Box::new(move |offer| {
                let sender = sender.clone();

                tokio::spawn(async move {
                    let _ = sender.send(EngineMessage::PublisherOffer { offer }).await;
                });

                Box::pin(async move {})
            })
        });

        let mut primary_pc = &mut publisher_pc;
        let mut secondary_pc = &mut subscriber_pc;
        if join.subscriber_primary {
            primary_pc = &mut subscriber_pc;
            secondary_pc = &mut publisher_pc;

            primary_pc.peer_connection().on_data_channel(Box::new({
                let sender = sender.clone();
                move |data_channel| {
                    let _ =
                        sender.blocking_send(EngineMessage::PrimaryDataChannel { data_channel });
                }
            }));
        }

        primary_pc.peer_connection().on_connection_change(Box::new({
            let sender = sender.clone();
            move |state| {
                let _ = sender.blocking_send(EngineMessage::ConnectionChange {
                    state,
                    primary: true,
                });
            }
        }));

        secondary_pc
            .peer_connection()
            .on_connection_change(Box::new({
                let sender = sender.clone();
                move |state| {
                    let _ = sender.blocking_send(EngineMessage::ConnectionChange {
                        state,
                        primary: false,
                    });
                }
            }));

        // Note that when subscriber_primary feature is enabled,
        // the subscriber uses his own data channels created by the server.
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

        Self::configure_dc(&mut lossy_dc, sender.clone());
        Self::configure_dc(&mut reliable_dc, sender.clone());

        Ok(Self {
            publisher_pc: Arc::new(Mutex::new(publisher_pc)),
            subscriber_pc: Arc::new(Mutex::new(subscriber_pc)),
            lossy_dc: Arc::new(Mutex::new(lossy_dc)),
            reliable_dc: Arc::new(Mutex::new(reliable_dc)),
            lossy_dc_sub: Default::default(),
            reliable_dc_sub: Default::default(),
            msg_sender: sender,
            join_response: Mutex::new(join),
            pc_state: AtomicU8::new(PCState::New as u8),
            has_published: AtomicBool::new(false),
            on_data_handler: Default::default(),
        })
    }

    /// Map the libwebrtc listeners to a mpsc channel
    #[tracing::instrument]
    fn configure_dc(
        data_channel: &mut DataChannel,
        sender: mpsc::Sender<EngineMessage>,
    ) {
        data_channel.on_message(Box::new(move |data, binary| {
            let _ = sender.blocking_send(EngineMessage::Data {
                data: data.to_vec(),
                binary,
            });
        }));
    }

    #[tracing::instrument]
    async fn ensure_publisher_connected(
        self: &Arc<Self>,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        if !self.join_response.lock().await.subscriber_primary {
            return Ok(());
        }

        let publisher = &self.publisher_pc;
        {
            let mut publisher = publisher.lock().await;
            if !publisher.is_connected()
                && publisher.peer_connection().ice_connection_state()
                != IceConnectionState::IceConnectionChecking
            {
                tokio::spawn({
                    let internal = self.clone();
                    async move {
                        let _ = internal.negotiate_publisher().await;
                    }
                });
            }
        }

        let dc = self.data_channel(kind);
        if dc.lock().await.state() == DataState::Open {
            return Ok(());
        }

        let res = time::timeout(MAX_ICE_CONNECT_TIMEOUT, async move {
            let mut interval = time::interval(Duration::from_millis(50));

            loop {
                if publisher.lock().await.is_connected()
                    && dc.lock().await.state() == DataState::Open
                {
                    break;
                }

                interval.tick().await;
            }
        })
            .await;

        if res.is_err() {
            let err =
                EngineError::Connection("could not establish publisher connection".to_string());
            event!(Level::ERROR, error = ?err);
            Err(err)
        } else {
            Ok(())
        }
    }

    #[tracing::instrument]
    async fn handle_signal(
        self: &Arc<Self>,
        signal: signal_response::Message,
        signal_client: Arc<SignalClient>,
    ) -> Result<(), EngineError> {
        match signal {
            signal_response::Message::Answer(answer) => {
                event!(Level::TRACE, "received answer for publisher: {:?}", answer);
                let sdp = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                self.publisher_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
            }
            signal_response::Message::Offer(offer) => {
                event!(Level::TRACE, "received offer for subscriber: {:?}", offer);
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;

                self.subscriber_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
                let answer = self.subscriber_pc
                    .lock()
                    .await
                    .peer_connection()
                    .create_answer(RTCOfferAnswerOptions::default())
                    .await?;
                self.subscriber_pc
                    .lock()
                    .await
                    .peer_connection()
                    .set_local_description(answer.clone())
                    .await?;

                tokio::spawn(async move {
                    let _ = signal_client.send(signal_request::Message::Answer(
                        proto::SessionDescription {
                            r#type: "answer".to_string(),
                            sdp: answer.to_string(),
                        },
                    )).await;
                });
            }
            signal_response::Message::Trickle(trickle) => {
                let json: IceCandidateJSON = serde_json::from_str(&trickle.candidate_init)?;
                let ice = IceCandidate::from(&json.sdpMid, json.sdpMLineIndex, &json.candidate)?;

                event!(
                    Level::TRACE,
                    "received ice_candidate ({:?}) - {:?}",
                    SignalTarget::from_i32(trickle.target).unwrap(),
                    ice
                );

                if trickle.target == SignalTarget::Publisher as i32 {
                    self.publisher_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                } else {
                    self.subscriber_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    #[tracing::instrument]
    pub async fn run(
        self: &Arc<Self>,
        mut receiver: mpsc::Receiver<EngineMessage>,
        signal_client: Arc<SignalClient>,
    ) {
        loop {
            tokio::select! {
                signal = signal_client.recv() => {
                    match signal {
                        Some(signal) => {
                            if let Err(err) = self.handle_signal(signal, signal_client.clone()).await {
                                event!(
                                    Level::ERROR,
                                    "failed to handle signal: {:?}",
                                    err,
                                );
                            }
                        }
                        None => {
                            // TODO(theomonnom) Trigger reconnect
                        }
                    }
                },
                Some(msg) = receiver.recv() => {
                    if let Err(err) = self.handle_message(msg, signal_client.clone()).await {
                        event!(
                            Level::ERROR,
                            "failed to handle engine message: {:?}",
                            err,
                        );
                    }
                },
            }
        }
    }

    #[tracing::instrument]
    async fn handle_message(
        self: &Arc<Self>,
        msg: EngineMessage,
        signal_client: Arc<SignalClient>,
    ) -> Result<(), EngineError> {
        match msg {
            EngineMessage::IceCandidate {
                ice_candidate,
                publisher,
            } => {
                let target = if publisher {
                    SignalTarget::Publisher
                } else {
                    SignalTarget::Subscriber
                };

                event!(
                    Level::TRACE,
                    "sending ice_candidate ({:?}) - {:?}",
                    target,
                    ice_candidate
                );

                let json = serde_json::to_string(&IceCandidateJSON {
                    sdpMid: ice_candidate.sdp_mid(),
                    sdpMLineIndex: ice_candidate.sdp_mline_index(),
                    candidate: ice_candidate.candidate(),
                })?;

                // Send the ice_candidate to the server
                tokio::spawn(async move {
                    let _ = signal_client.send(signal_request::Message::Trickle(
                        TrickleRequest {
                            candidate_init: json,
                            target: target as i32,
                        },
                    )).await;
                });
            }
            EngineMessage::ConnectionChange { state, primary } => {
                if primary && state == PeerConnectionState::Connected {
                    let old_state = self.pc_state.load(Ordering::SeqCst);
                    self.pc_state
                        .store(PCState::Connected as u8, Ordering::SeqCst);

                    if old_state == PCState::New as u8 {
                        // TODO(theomonnom) OnConnected
                    }
                } else if state == PeerConnectionState::Failed {
                    self.pc_state.store(PCState::Disconnected as u8, Ordering::SeqCst);

                    // TODO(theomonnom) handle Disconnect
                }
            }
            EngineMessage::PrimaryDataChannel { mut data_channel } => {
                let reliable = data_channel.label() == RELIABLE_DC_LABEL;
                Self::configure_dc(&mut data_channel, self.msg_sender.clone());

                event!(
                    Level::TRACE,
                    "received subscriber data_channel - {:?}",
                    data_channel
                );

                if reliable {
                    *self.reliable_dc_sub.lock().await = Some(data_channel);
                } else {
                    *self.lossy_dc_sub.lock().await = Some(data_channel);
                }
            }
            EngineMessage::PublisherOffer { offer } => {
                event!(
                    Level::TRACE,
                    "sending publisher offer - {:?}",
                    offer
                );

                // Send the offer to the server
                tokio::spawn(async move {
                    let _ = signal_client.send(signal_request::Message::Offer(
                        proto::SessionDescription {
                            r#type: "offer".to_string(),
                            sdp: offer.to_string(),
                        },
                    )).await;
                });
            }
            EngineMessage::Data {
                data,
                binary,
            } => {
                if !binary {
                    return Err(EngineError::Internal(
                        "text messages aren't supported by LiveKit".to_string(),
                    ));
                }

                let data = DataPacket::decode(&*data)?;
                match data.value.unwrap() {
                    Value::User(user) => {
                        let mut handler = self.on_data_handler.lock().await;
                        if let Some(f) = &mut *handler {
                            f(Packet {
                                data: user,
                                kind: data_packet::Kind::from_i32(data.kind).unwrap(),
                            })
                                .await;
                        }
                    }
                    Value::Speaker(_) => {
                        // TODO(theomonnonm)
                    }
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument]
    async fn negotiate_publisher(self: &Arc<Self>) -> Result<(), EngineError> {
        self.has_published.store(true, Ordering::SeqCst);
        if let Err(err) = self.publisher_pc.lock().await.negotiate().await {
            event!(
                Level::ERROR,
                "failed to negotiate the publisher: {:?}",
                err,
            );
            Err(EngineError::Rtc(err))
        } else {
            Ok(())
        }
    }

    fn data_channel(&self, kind: data_packet::Kind) -> Arc<Mutex<DataChannel>> {
        if kind == data_packet::Kind::Reliable {
            self.reliable_dc.clone()
        } else {
            self.lossy_dc.clone()
        }
    }
}

impl Debug for EngineInternal {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "EngineInternal")
    }
}
