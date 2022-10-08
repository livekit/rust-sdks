use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use prost::Message;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::{event, Level};

use livekit_webrtc::data_channel::{DataChannel, DataChannelInit, DataState};
use livekit_webrtc::jsep::{IceCandidate, SessionDescription};
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnectionState, RTCOfferAnswerOptions,
};
use livekit_webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};

use crate::lk_runtime::LKRuntime;
use crate::pc_transport::PCTransport;
use crate::proto;
use crate::proto::data_packet::Value;
use crate::proto::{
    data_packet, signal_request, signal_response, DataPacket, JoinResponse, SignalTarget,
    TrickleRequest,
};
use crate::rtc_engine::{EngineError, MAX_ICE_CONNECT_TIMEOUT};
use crate::signal_client::SignalClient;

const LOSSY_DC_LABEL: &str = "_lossy";
const RELIABLE_DC_LABEL: &str = "_reliable";

// Used to communicate IceCandidate with the server
#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct IceCandidateJSON {
    sdpMid: String,
    sdpMLineIndex: i32,
    candidate: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PCState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

#[derive(Debug)]
pub(crate) enum InternalMessage {
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

pub(crate) struct EngineInternal {
    pub(super) publisher_pc: Arc<Mutex<PCTransport>>,
    pub(super) subscriber_pc: Arc<Mutex<PCTransport>>,
    pub(super) lossy_dc: Arc<Mutex<DataChannel>>,
    pub(super) reliable_dc: Arc<Mutex<DataChannel>>,
    pub(super) lossy_dc_sub: Arc<Mutex<Option<DataChannel>>>,
    pub(super) reliable_dc_sub: Arc<Mutex<Option<DataChannel>>>,

    pub(super) msg_sender: mpsc::Sender<InternalMessage>,
    pub(super) join_response: Mutex<JoinResponse>,
    pub(super) pc_state: AtomicU8, // casted to PCState
    pub(super) has_published: AtomicBool,
}

impl Debug for EngineInternal {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "EngineInternal")
    }
}

impl EngineInternal {
    /// Configure the PeerConnections
    ///
    /// This is called on connect & on full reconnect.
    /// Create the PeerConnections & the DataChannels.
    /// Register listeners and send the internal messages
    /// to the event_loop.
    #[tracing::instrument]
    pub(super) fn configure(
        lk_runtime: Arc<LKRuntime>,
        sender: mpsc::Sender<InternalMessage>,
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
                let _ = sender.blocking_send(InternalMessage::IceCandidate {
                    ice_candidate,
                    publisher: true,
                });
            }
        }));

        subscriber_pc.peer_connection().on_ice_candidate(Box::new({
            let sender = sender.clone();
            move |ice_candidate| {
                let _ = sender.blocking_send(InternalMessage::IceCandidate {
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
                    let _ = sender.send(InternalMessage::PublisherOffer { offer }).await;
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
                        sender.blocking_send(InternalMessage::PrimaryDataChannel { data_channel });
                }
            }));
        }

        primary_pc.peer_connection().on_connection_change(Box::new({
            let sender = sender.clone();
            move |state| {
                let _ = sender.blocking_send(InternalMessage::ConnectionChange {
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
                    let _ = sender.blocking_send(InternalMessage::ConnectionChange {
                        state,
                        primary: false,
                    });
                }
            }));

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
        })
    }

    /// Send InternalMessage when a datachannel receives data
    #[tracing::instrument]
    fn configure_dc(data_channel: &mut DataChannel, sender: mpsc::Sender<InternalMessage>) {
        data_channel.on_message(Box::new(move |data, binary| {
            let _ = sender.blocking_send(InternalMessage::Data {
                data: data.to_vec(),
                binary,
            });
        }));
    }

    /// Ensure the publisher PeerConnection is connected
    ///
    /// When subscriber_primary is enabled, only the subscriber PeerConnection is negotiated.
    /// This allows for faster connection when we don't need the publisher
    #[tracing::instrument]
    pub(super) async fn ensure_publisher_connected(
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

    /// Run the event_loop of the RTCEngine
    #[tracing::instrument]
    pub(super) async fn run(
        self: &Arc<Self>,
        mut receiver: mpsc::Receiver<InternalMessage>,
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

    /// Handle SignalResponse messages coming from the server
    ///
    /// Run the needed livekit-protocol
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
                // Handle the subscriber offer & send an answer to livekit-server
                // We always get an offer from the server when connecting
                event!(Level::TRACE, "received offer for subscriber: {:?}", offer);
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;

                self.subscriber_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
                let answer = self
                    .subscriber_pc
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
                    let _ = signal_client
                        .send(signal_request::Message::Answer(proto::SessionDescription {
                            r#type: "answer".to_string(),
                            sdp: answer.to_string(),
                        }))
                        .await;
                });
            }
            signal_response::Message::Trickle(trickle) => {
                // Add the IceCandidate received from the livekit-server
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

    /// Handle libwebrtc messages
    ///
    /// Every message used inside this function comes from libwebrtc.
    /// The messages are received in [EngineInternal](#run)
    /// We're not handling the messages inside the signaling_thread, to return
    /// as quickly as possible.
    #[tracing::instrument]
    async fn handle_message(
        self: &Arc<Self>,
        msg: InternalMessage,
        signal_client: Arc<SignalClient>,
    ) -> Result<(), EngineError> {
        match msg {
            InternalMessage::IceCandidate {
                ice_candidate,
                publisher,
            } => {
                // Send the IceCandidate to livekit-server
                // Note that ContinualGatheringPolicy is set to GatherContinually
                let json = serde_json::to_string(&IceCandidateJSON {
                    sdpMid: ice_candidate.sdp_mid(),
                    sdpMLineIndex: ice_candidate.sdp_mline_index(),
                    candidate: ice_candidate.candidate(),
                })?;

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

                tokio::spawn(async move {
                    let _ = signal_client
                        .send(signal_request::Message::Trickle(TrickleRequest {
                            candidate_init: json,
                            target: target as i32,
                        }))
                        .await;
                });
            }
            InternalMessage::ConnectionChange { state, primary } => {
                // PeerConnectionState changed
                // Reconnect if we've been disconnected unexpectedly
                // If connected for the first time, send OnConnect event
                if primary && state == PeerConnectionState::Connected {
                    let old_state = self.pc_state.load(Ordering::SeqCst);
                    self.pc_state
                        .store(PCState::Connected as u8, Ordering::SeqCst);

                    if old_state == PCState::New as u8 {
                        // TODO(theomonnom) OnConnected
                    }
                } else if state == PeerConnectionState::Failed {
                    self.pc_state
                        .store(PCState::Disconnected as u8, Ordering::SeqCst);

                    // TODO(theomonnom) handle Disconnect
                }
            }
            InternalMessage::PrimaryDataChannel { mut data_channel } => {
                // Received datachannel from the primary PeerConnection.
                // If subscriber_primary is enabled, the datachannel is used for downstream data
                let reliable = data_channel.label() == RELIABLE_DC_LABEL;
                Self::configure_dc(&mut data_channel, self.msg_sender.clone());

                event!(
                    Level::TRACE,
                    "received primary data_channel - {:?}",
                    data_channel
                );

                if reliable {
                    *self.reliable_dc_sub.lock().await = Some(data_channel);
                } else {
                    *self.lossy_dc_sub.lock().await = Some(data_channel);
                }
            }
            InternalMessage::PublisherOffer { offer } => {
                // Send the publisher offer to livekit-server
                event!(Level::TRACE, "sending publisher offer - {:?}", offer);

                tokio::spawn(async move {
                    let _ = signal_client
                        .send(signal_request::Message::Offer(proto::SessionDescription {
                            r#type: "offer".to_string(),
                            sdp: offer.to_string(),
                        }))
                        .await;
                });
            }
            InternalMessage::Data { data, binary } => {
                // Received data from a datachannel
                // If this is a Speaker DataPacket, update the active speakers
                // Send SpeakersChanged/OnData event
                if !binary {
                    return Err(EngineError::Internal(
                        "text messages aren't supported".to_string(),
                    ));
                }

                let data = DataPacket::decode(&*data)?;
                match data.value.unwrap() {
                    Value::User(user) => {
                        /*let mut handler = self.on_data_handler.lock().await;
                        if let Some(f) = &mut *handler {
                            f(Packet {
                                data: user,
                                kind: data_packet::Kind::from_i32(data.kind).unwrap(),
                            })
                                .await;
                        }*/
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
            event!(Level::ERROR, "failed to negotiate the publisher: {:?}", err,);
            Err(EngineError::Rtc(err))
        } else {
            Ok(())
        }
    }

    pub(super) fn data_channel(&self, kind: data_packet::Kind) -> Arc<Mutex<DataChannel>> {
        if kind == data_packet::Kind::Reliable {
            self.reliable_dc.clone()
        } else {
            self.lossy_dc.clone()
        }
    }
}
