use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use lazy_static::lazy_static;
use log::{error, trace};
use prost::Message as ProstMessage;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::protocol::frame::coding::Data;

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
use crate::proto::{
    DataPacket, JoinResponse, signal_request, signal_response, SignalTarget, TrickleRequest,
};
use crate::signal_client::{SignalClient, SignalError};

const LOSSY_DC_LABEL: &str = "_lossy";
const RELIABLE_DC_LABEL: &str = "_reliable";

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

#[derive(PartialEq, Debug, Copy, Clone)]
enum PCState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

enum EngineMessage {}

struct PeerInternal {
    publisher_pc: PCTransport,
    subscriber_pc: PCTransport,

    lossy_dc: DataChannel,
    reliable_dc: DataChannel,

    pub_ice_rx: mpsc::Receiver<IceCandidate>,
    sub_ice_rx: mpsc::Receiver<IceCandidate>,

    pub_offer_rx: mpsc::Receiver<SessionDescription>,

    primary_connection_state_rx: mpsc::Receiver<PeerConnectionState>,
    secondary_connection_state_rx: mpsc::Receiver<PeerConnectionState>,

    lossy_data_rx: mpsc::Receiver<DataPacket>,
    reliable_data_rx: mpsc::Receiver<DataPacket>,

    sub_dc_rx: mpsc::Receiver<DataChannel>,

    pc_state: PCState,
}

struct RTCInternal {
    #[allow(unused)]
    lk_runtime: Arc<LKRuntime>,
    signal_client: Arc<SignalClient>,
    pc_internal: PeerInternal,
}

impl RTCInternal {
    async fn connect(url: &str, token: &str) -> Result<Self, EngineError> {
        let mut lk_runtime = None;
        {
            // Acquire an existing/a new LKRuntime
            let mut lk_runtime_ref = LK_RUNTIME.lock().unwrap();
            lk_runtime = lk_runtime_ref.upgrade();

            if lk_runtime.is_none() {
                let new_runtime = Arc::new(LKRuntime::new());
                *lk_runtime_ref = Arc::downgrade(&new_runtime);
                lk_runtime = Some(new_runtime);
            }
        }
        let lk_runtime = lk_runtime.unwrap();
        let signal_client = Arc::new(signal_client::connect(url, token).await?);

        trace!("waiting join_response..");
        if let signal_response::Message::Join(join) = signal_client.recv().await? {
            trace!("configuring peer_connections: {:?}", join);
            let mut pc_internal = Self::configure(lk_runtime.clone(), join.clone())?;

            if !join.subscriber_primary {
                pc_internal.publisher_pc.negotiate().await?;
            }

            Ok(Self {
                lk_runtime,
                signal_client,
                pc_internal,
            })
        } else {
            panic!("the first received message isn't a JoinResponse");
        }
    }

    fn request_signal(&mut self, msg: signal_request::Message) {
        tokio::spawn({
            let sc = self.signal_client.clone();

            async move {
                if let Err(err) = sc.send(msg).await {
                    error!("failed to send signal: {:?}", err);
                }
            }
        });
    }

    async fn handle_signal(&mut self, signal: signal_response::Message) -> Result<(), EngineError> {
        match signal {
            signal_response::Message::Answer(answer) => {
                let sdp = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                self.pc_internal.publisher_pc.set_remote_description(sdp).await?;
            },
            signal_response::Message::Offer(offer) => {
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;
                self.pc_internal.subscriber_pc.set_remote_description(sdp).await?;
                let answer = self.pc_internal.subscriber_pc.peer_connection().create_answer(RTCOfferAnswerOptions::default()).await?;
                self.pc_internal.subscriber_pc.peer_connection().set_local_description(answer.clone()).await?;

                self.request_signal(signal_request::Message::Answer(proto::SessionDescription {
                    r#type: "answer".to_string(),
                    sdp: answer.to_string(),
                }));
            },
            signal_response::Message::Trickle(trickle) => {
                let json: serde_json::Value = serde_json::from_str(&trickle.candidate_init)?;
                let ice = IceCandidate::from(
                    json["sdpMid"].as_str().unwrap(),
                    json["sdpMLineIndex"].as_i64().unwrap().try_into().unwrap(),
                    json["candidate"].as_str().unwrap()
                )?;

                if trickle.target == SignalTarget::Publisher as i32 {
                    self.pc_internal.publisher_pc.add_ice_candidate(ice).await?;
                } else {
                    self.pc_internal.subscriber_pc.add_ice_candidate(ice).await?;
                }
            }
            _ => {},
        }

        Ok(())
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Ok(signal) = self.signal_client.recv() => {
                    if let Err(err) = self.handle_signal(signal).await {
                        error!("failed to handle signal: {:?}", err);
                    }
                },
                Some(ice_candidate) = self.pc_internal.pub_ice_rx.recv() => {
                    self.request_signal(signal_request::Message::Trickle(TrickleRequest {
                        candidate_init: ice_candidate.to_string(),
                        target: SignalTarget::Publisher as i32
                    }));
                },
                Some(ice_candidate) = self.pc_internal.sub_ice_rx.recv() => {
                    self.request_signal(signal_request::Message::Trickle(TrickleRequest {
                        candidate_init: ice_candidate.to_string(),
                        target: SignalTarget::Subscriber as i32
                    }));
                },
                Some(sdp) = self.pc_internal.pub_offer_rx.recv() => {
                    trace!("received publisher offer: {:?}", sdp);
                    self.request_signal(signal_request::Message::Offer(proto::SessionDescription {
                        r#type: "offer".to_string(),
                        sdp: sdp.to_string(),
                    }));
                },
                Some(state) = self.pc_internal.primary_connection_state_rx.recv() => {
                    if state == PeerConnectionState::Connected {
                        let old_state = self.pc_internal.pc_state;
                        self.pc_internal.pc_state = PCState::Connected;

                        if old_state == PCState::New {
                            // TODO(theomonnom) OnConnected
                        }
                    } else if state == PeerConnectionState::Failed {
                        self.pc_internal.pc_state = PCState::Disconnected;
                        // TODO(theomonnom) Handle Disconnect
                    }
                },
                Some(state) = self.pc_internal.secondary_connection_state_rx.recv() => {
                    if state == PeerConnectionState::Failed {
                        self.pc_internal.pc_state = PCState::Disconnected;
                        // TODO(theomonnom) Handle Disconnect
                    }
                },
                Some(data) = self.pc_internal.lossy_data_rx.recv() => {

                },
                Some(data) = self.pc_internal.reliable_data_rx.recv() => {

                },
                Some(mut dc) = self.pc_internal.sub_dc_rx.recv() => {
                    // Subscriber DataChannels
                    // Only received when the subscriber_primary is enabled
                    trace!("using subscriber data channels");

                    let (data_tx, data_rx) = mpsc::channel(8);
                    Self::configure_dc(&mut dc, data_tx);

                    if dc.label() == RELIABLE_DC_LABEL {
                        self.pc_internal.reliable_dc = dc;
                        self.pc_internal.reliable_data_rx = data_rx;
                    } else {
                        self.pc_internal.lossy_dc = dc;
                        self.pc_internal.lossy_data_rx = data_rx;
                    }
                }
            }
        }
    }

    fn configure_dc(data_channel: &mut DataChannel, data_tx: mpsc::Sender<DataPacket>) {
        let label = data_channel.label();
        data_channel.on_message(Box::new(move |data, _| {
            if let Ok(data) = DataPacket::decode(data) {
                let _ = data_tx.blocking_send(data);
            } else {
                trace!("{} - failed to decode DataPacket", label);
            }
        }));
    }

    fn configure(
        lk_runtime: Arc<LKRuntime>,
        join: JoinResponse,
    ) -> Result<PeerInternal, EngineError> {
        let cfg = RTCConfiguration {
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

        // Create the PeerConnections
        let mut publisher_pc = PCTransport::new(lk_runtime.clone(), cfg.clone())?;
        let mut subscriber_pc = PCTransport::new(lk_runtime, cfg)?;

        let (pub_ice_tx, pub_ice_rx) = mpsc::channel(8);
        let (sub_ice_tx, sub_ice_rx) = mpsc::channel(8);
        let (pub_offer_tx, pub_offer_rx) = mpsc::channel(8);
        let (primary_connection_state_tx, primary_connection_state_rx) = mpsc::channel(8);
        let (secondary_connection_state_tx, secondary_connection_state_rx) = mpsc::channel(8);
        let (lossy_data_tx, lossy_data_rx) = mpsc::channel(8);
        let (reliable_data_tx, reliable_data_rx) = mpsc::channel(8);
        let (sub_dc_tx, sub_dc_rx) = mpsc::channel(8);

        publisher_pc
            .peer_connection()
            .on_ice_candidate(Box::new(move |ice_candidate| {
                trace!("publisher - on_ice_candidate: {:?}", ice_candidate);
                let _ = pub_ice_tx.blocking_send(ice_candidate);
            }));

        subscriber_pc
            .peer_connection()
            .on_ice_candidate(Box::new(move |ice_candidate| {
                trace!("subscriber - on_ice_candidate: {:?}", ice_candidate);
                let _ = sub_ice_tx.blocking_send(ice_candidate);
            }));

        publisher_pc.on_offer(Box::new(move |offer| {
            trace!("publisher - on_offer: {:?}", offer);
            let _ = pub_offer_tx.blocking_send(offer); // TODO(theomonnom) Don't use blocking_send here
        }));

        let mut primary_pc = &mut publisher_pc;
        let mut secondary_pc = &mut subscriber_pc;
        if join.subscriber_primary {
            primary_pc = &mut subscriber_pc;
            secondary_pc = &mut publisher_pc;

            primary_pc.peer_connection().on_data_channel(Box::new(move |dc| {
                let _ = sub_dc_tx.blocking_send(dc);
            }));
        }

        primary_pc
            .peer_connection()
            .on_connection_change(Box::new(move |state| {
                let _ = primary_connection_state_tx.blocking_send(state);
            }));

        secondary_pc
            .peer_connection()
            .on_connection_change(Box::new(move |state| {
                let _ = secondary_connection_state_tx.blocking_send(state);
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

        Self::configure_dc(&mut lossy_dc, lossy_data_tx);
        Self::configure_dc(&mut reliable_dc, reliable_data_tx);

        Ok(PeerInternal {
            publisher_pc,
            subscriber_pc,
            lossy_dc,
            reliable_dc,
            pub_ice_rx,
            sub_ice_rx,
            pub_offer_rx,
            primary_connection_state_rx,
            secondary_connection_state_rx,
            lossy_data_rx,
            reliable_data_rx,
            sub_dc_rx,
            pc_state: PCState::New,
        })
    }
}

pub struct RTCEngine {}

/// Initialize the SignalClient & the PeerConnections
pub async fn connect(url: &str, token: &str) -> Result<RTCEngine, EngineError> {
    let mut rtc_internal = RTCInternal::connect(url, token).await?;
    tokio::spawn(async move {
        rtc_internal.run().await
    });

    Ok(RTCEngine{})
}

impl RTCEngine {
    async fn rtc_handle() {
        loop {}
    }
}

#[tokio::test]
async fn test_test() {
    env_logger::init();

    let engine = connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NzEyMzk4NjAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ0ZXN0IiwibmJmIjoxNjY0MDM5ODYwLCJzdWIiOiJ0ZXN0IiwidmlkZW8iOnsicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZX19.0Bee2jI2cSZveAbZ8MLc-ADoMYQ4l8IRxcAxpXAS6a8").await.unwrap();


    sleep(Duration::from_secs(60)).await;

}

/*sync fn handle_rtc(mut signal_receiver: broadcast::Receiver<Message>) {
    loop {
        let msg = match signal_receiver.recv().await {
            Ok(msg) => msg,
            Err(error) => {
                error!("Failed to receive SignalResponse: {:?}", error);
                continue;
            }
        };

        match msg {
            Message::Join(join) => {}
            Message::Trickle(trickle) => {}
            Message::Answer(answer) => {}
            Message::Offer(offer) => {}
            _ => {}
        }
    }
}*/
