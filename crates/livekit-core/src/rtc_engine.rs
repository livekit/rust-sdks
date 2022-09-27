use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use lazy_static::lazy_static;
use log::{error, trace};
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

use livekit_webrtc::data_channel::{DataChannel, DataChannelInit};
use livekit_webrtc::jsep::{IceCandidate, SdpParseError, SessionDescription};
use livekit_webrtc::peer_connection::{PeerConnectionState, RTCOfferAnswerOptions};
use livekit_webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};
use livekit_webrtc::rtc_error::RTCError;

use crate::{proto, signal_client};
use crate::lk_runtime::LKRuntime;
use crate::pc_transport::PCTransport;
use crate::proto::{DataPacket, JoinResponse, signal_request, signal_response, SignalResponse, SignalTarget, TrickleRequest};
use crate::signal_client::{SignalClient, SignalError};

const LOSSY_DC_LABEL: &str = "_lossy";
const RELIABLE_DC_LABEL: &str = "_reliable";

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

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
        publisher: bool
    },
    ConnectionChange {
        state: PeerConnectionState,
        primary: bool
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
        reliable: bool
    }
}

struct EngineInternal {
    publisher_pc: Arc<Mutex<PCTransport>>,
    subscriber_pc: Arc<Mutex<PCTransport>>,

    lossy_dc: Arc<Mutex<DataChannel>>,
    reliable_dc: Arc<Mutex<DataChannel>>,

    msg_sender: mpsc::Sender<EngineMessage>,

    pc_state: AtomicU8, // PCState
}

pub struct RTCEngine {
    #[allow(unused)]
    lk_runtime: Arc<LKRuntime>, // Keep a reference while we're using the RTCEngine
    signal_client: Arc<SignalClient>,
    internal: Arc<EngineInternal>
}

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

    if let signal_response::Message::Join(join_response) = signal_client.recv().await? {
        trace!("received join_response: {:?}", join_response);
        let (sender, receiver) = mpsc::channel(8);
        let internal = Arc::new(RTCEngine::configure(lk_runtime.clone(), sender, join_response.clone())?);

        if !join_response.subscriber_primary {
            internal.publisher_pc.lock().await.negotiate().await?;
        }

        tokio::spawn({
            let signal_client = signal_client.clone();
            let internal = internal.clone();

            async move {
                RTCEngine::handle_loop(receiver, signal_client, internal).await;
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

    async fn send_data() {

    }

    fn send_request(msg: signal_request::Message, signal_client: Arc<SignalClient>) {
        tokio::spawn(async move {
            if let Err(err) = signal_client.send(msg).await {
                error!("failed to send signal: {:?}", err);
            }
        });
    }

    async fn handle_signal(signal: signal_response::Message, signal_client: &Arc<SignalClient>, rtc_internal: &Arc<EngineInternal>) -> Result<(), EngineError> {
        match signal {
            signal_response::Message::Answer(answer) => {
                let sdp = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                rtc_internal.publisher_pc.lock().await.set_remote_description(sdp).await?;
            },
            signal_response::Message::Offer(offer) => {
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;
                let mut subscriber_pc = rtc_internal.subscriber_pc.lock().await;

                subscriber_pc.set_remote_description(sdp).await?;
                let answer = subscriber_pc.peer_connection().create_answer(RTCOfferAnswerOptions::default()).await?;
                subscriber_pc.peer_connection().set_local_description(answer.clone()).await?;

                Self::send_request(signal_request::Message::Answer(proto::SessionDescription {
                    r#type: "answer".to_string(),
                    sdp: answer.to_string(),
                }), signal_client.clone());
            },
            signal_response::Message::Trickle(trickle) => {
                let json: serde_json::Value = serde_json::from_str(&trickle.candidate_init)?;
                let ice = IceCandidate::from(
                    json["sdpMid"].as_str().unwrap(),
                    json["sdpMLineIndex"].as_i64().unwrap().try_into().unwrap(),
                    json["candidate"].as_str().unwrap()
                )?;

                if trickle.target == SignalTarget::Publisher as i32 {
                    rtc_internal.publisher_pc.lock().await.add_ice_candidate(ice).await?;
                } else {
                    rtc_internal.subscriber_pc.lock().await.add_ice_candidate(ice).await?;
                }
            }
            _ => {},
        }

        Ok(())
    }

    async fn handle_loop(mut receiver: mpsc::Receiver<EngineMessage>, signal_client: Arc<SignalClient>, rtc_internal: Arc<EngineInternal>) {
        loop {
            tokio::select! {
                Ok(signal) = signal_client.recv() => {
                    if let Err(err) = Self::handle_signal(signal, &signal_client, &rtc_internal).await {
                        error!("failed to handle signal: {:?}", err);
                    }
                },
                Some(msg) = receiver.recv() => {
                    match msg {
                        EngineMessage::IceCandidate { ice_candidate, publisher } => {
                            trace!("received ice_candidate: {:?} (publisher: {:?})", ice_candidate, publisher);
                            // Send the ice_candidate to the server
                            Self::send_request(signal_request::Message::Trickle(TrickleRequest {
                                candidate_init: ice_candidate.to_string(),
                                target: if publisher {SignalTarget::Publisher} else {SignalTarget::Subscriber} as i32
                            }), signal_client.clone());
                        }
                        EngineMessage::ConnectionChange { state, primary } => {
                            if primary && state == PeerConnectionState::Connected {
                                let old_state = rtc_internal.pc_state.load(Ordering::SeqCst);
                                rtc_internal.pc_state.store(PCState::Connected as u8, Ordering::SeqCst);

                                if old_state == PCState::New as u8 {
                                    // TODO(theomonnom) OnConnected
                                }
                            } else if state == PeerConnectionState::Failed {
                                rtc_internal.pc_state.store(PCState::Disconnected as u8, Ordering::SeqCst);
                                // TODO(theomonnom) handle Disconnect
                            }
                        }
                        EngineMessage::PrimaryDataChannel { mut data_channel } => {
                            let reliable = data_channel.label() == RELIABLE_DC_LABEL;
                            Self::configure_dc(&mut data_channel, reliable, rtc_internal.msg_sender.clone());

                            trace!("received and using subscriber datachannel (reliable: {:?})", reliable);
                            if reliable {
                                *rtc_internal.reliable_dc.lock().await = data_channel;
                            } else {
                                *rtc_internal.lossy_dc.lock().await = data_channel;
                            }
                        }
                        EngineMessage::PublisherOffer { offer } => {
                            trace!("received publisher offer: {:?}", offer);
                            // Send the offer to the server
                            Self::send_request(signal_request::Message::Offer(proto::SessionDescription {
                                r#type: "offer".to_string(),
                                sdp: offer.to_string(),
                            }), signal_client.clone());
                        }
                        EngineMessage::Data { data, binary, reliable } => {}
                    }
                }
            }
        }
    }

    /// This function is called on connect & on reconnect
    /// It creates the PeerConnections, the DataChannels & the libwebrtc listeners
    fn configure(lk_runtime: Arc<LKRuntime>, sender: mpsc::Sender<EngineMessage>, join: JoinResponse) -> Result<EngineInternal, EngineError> {
        let rtc_config = RTCConfiguration {
            ice_servers: {
                let mut servers = vec![];
                for is in join.ice_servers {
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

        let mut publisher_pc = PCTransport::new(lk_runtime.pc_factory.create_peer_connection(rtc_config.clone())?);
        let mut subscriber_pc = PCTransport::new(lk_runtime.pc_factory.create_peer_connection(rtc_config)?);

        publisher_pc.peer_connection().on_ice_candidate(Box::new({
            let sender = sender.clone();
            move |ice_candidate| {
                let _ = sender.blocking_send(EngineMessage::IceCandidate {
                    ice_candidate,
                    publisher: true
                });
            }
        }));

        subscriber_pc.peer_connection().on_ice_candidate(Box::new({
            let sender = sender.clone();
            move |ice_candidate| {
                let _ = sender.blocking_send(EngineMessage::IceCandidate {
                    ice_candidate,
                    publisher: false
                });
            }
        }));

        publisher_pc.on_offer(Box::new({
            let sender = sender.clone();
            move |offer| {
                let _ = sender.blocking_send(EngineMessage::PublisherOffer {offer});
            }
        }));

        let mut primary_pc = &mut publisher_pc;
        let mut secondary_pc = &mut subscriber_pc;
        if join.subscriber_primary {
            primary_pc = &mut subscriber_pc;
            secondary_pc = &mut publisher_pc;

            primary_pc.peer_connection().on_data_channel(Box::new({{
                let sender = sender.clone();
                move |data_channel| {
                    let _ = sender.blocking_send(EngineMessage::PrimaryDataChannel {data_channel});
                }
            }}));
        }

        primary_pc
            .peer_connection()
            .on_connection_change(Box::new({
                let sender = sender.clone();
                move |state| {
                    let _ = sender.blocking_send(EngineMessage::ConnectionChange {
                        state,
                        primary: true
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
                        primary: false
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

        Self::configure_dc(&mut lossy_dc, true, sender.clone());
        Self::configure_dc(&mut reliable_dc, false, sender.clone());

        Ok(EngineInternal {
            publisher_pc: Arc::new(Mutex::new(publisher_pc)),
            subscriber_pc: Arc::new(Mutex::new(subscriber_pc)),
            lossy_dc: Arc::new(Mutex::new(lossy_dc)),
            reliable_dc: Arc::new(Mutex::new(reliable_dc)),
            pc_state: AtomicU8::new(PCState::New as u8),
            msg_sender: sender
        })
    }

    /// Map the libwebrtc listeners to a mpsc channel
    fn configure_dc(data_channel: &mut DataChannel, reliable: bool, sender: mpsc::Sender<EngineMessage>) {
        data_channel.on_message(Box::new(move |data, binary| {
            let _ = sender.blocking_send(EngineMessage::Data {
                data: data.to_vec(),
                reliable,
                binary
            });
        }));
    }
}

#[tokio::test]
async fn test_test() {
    env_logger::init();
    let engine = connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NzEyMzk4NjAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ0ZXN0IiwibmJmIjoxNjY0MDM5ODYwLCJzdWIiOiJ0ZXN0IiwidmlkZW8iOnsicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZX19.0Bee2jI2cSZveAbZ8MLc-ADoMYQ4l8IRxcAxpXAS6a8").await.unwrap();

    sleep(Duration::from_secs(60)).await;
}
