use std::sync::{Arc, Mutex, Weak};

use lazy_static::lazy_static;
use log::{error, trace};
use prost::Message as ProstMessage;
use thiserror::Error;
use tokio::sync::mpsc;

use livekit_webrtc::data_channel::{DataChannel, DataChannelInit};
use livekit_webrtc::jsep::{IceCandidate, SessionDescription};
use livekit_webrtc::peer_connection::PeerConnectionState;
use livekit_webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};
use livekit_webrtc::rtc_error::RTCError;

use crate::lk_runtime::LKRuntime;
use crate::pc_transport::PCTransport;
use crate::proto::{DataPacket, JoinResponse, signal_request, SignalTarget, TrickleRequest};
use crate::proto::signal_response::Message;
use crate::signal_client;
use crate::signal_client::{SignalClient, SignalError};

const LOSSY_DC_LABEL: &str = "_lossy";
const RELIABLE_DC_LABEL: &str = "_reliable";

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("signal failure")]
    SignalError(#[from] SignalError),
    #[error("internal webrtc failure")]
    RTCError(#[from] RTCError),
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
}

struct RTCInternal {
    lk_runtime: Arc<LKRuntime>,
    signal_client: Arc<SignalClient>,
    pc_internal: PeerInternal,
}

impl RTCInternal {
    async fn connect(
        url: &str,
        token: &str,
    ) -> Result<Self, EngineError> {
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

        if let Message::Join(join) = signal_client.recv().await? {
            let pc_internal = Self::configure(lk_runtime.clone(), join)?;

            Ok(Self {
                lk_runtime,
                signal_client,
                pc_internal,
            })
        } else {
            panic!("the first received message isn't a JoinResponse");
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Ok(signal) = self.signal_client.recv() => {

                },
                Some(ice_candidate) = self.pc_internal.pub_ice_rx.recv() => {
                    tokio::spawn({
                        let sc = self.signal_client.clone();

                        async move {
                            let _ = sc.send(signal_request::Message::Trickle(TrickleRequest {
                                candidate_init: ice_candidate.to_string(),
                                target: SignalTarget::Publisher as i32
                            })).await;
                        }
                    });
                },
                Some(ice_candidate) = self.pc_internal.sub_ice_rx.recv() => {
                    tokio::spawn({
                        let sc = self.signal_client.clone();

                        async move {
                            let _ = sc.send(signal_request::Message::Trickle(TrickleRequest {
                                candidate_init: ice_candidate.to_string(),
                                target: SignalTarget::Subscriber as i32
                            })).await;
                        }
                    });
                },
                Some(sdp) = self.pc_internal.pub_offer_rx.recv() => {

                },
                Some(state) = self.pc_internal.primary_connection_state_rx.recv() => {

                },
                Some(state) = self.pc_internal.secondary_connection_state_rx.recv() => {

                },
                Some(data) = self.pc_internal.lossy_data_rx.recv() => {

                },
                Some(data) = self.pc_internal.reliable_data_rx.recv() => {

                }
            }
        }
    }

    fn configure(lk_runtime: Arc<LKRuntime>, join: JoinResponse) -> Result<PeerInternal, EngineError> {
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

        publisher_pc.peer_connection().on_ice_candidate(Box::new(move |ice_candidate| {
            trace!("publisher - on_ice_candidate: {:?}", ice_candidate);
            let _ = pub_ice_tx.blocking_send(ice_candidate);
        }));

        subscriber_pc.peer_connection().on_ice_candidate(Box::new(move |ice_candidate| {
            trace!("subscriber - on_ice_candidate: {:?}", ice_candidate);
            let _ = sub_ice_tx.blocking_send(ice_candidate);
        }));

        publisher_pc.on_offer(Box::new(move |offer| {
            trace!("publisher - on_offer: {:?}", offer);
            let _ = pub_offer_tx.blocking_send(offer); // TODO(theomonnom) Don't use blocking_send here
        }));

        let mut primary_pc = &publisher_pc;
        let mut secondary_pc = &subscriber_pc;
        if join.subscriber_primary {
            primary_pc = &subscriber_pc;
            secondary_pc = &publisher_pc;
        }

        primary_pc.peer_connection().on_connection_change(Box::new(move |state| {
            let _ = primary_connection_state_tx.blocking_send(state);
        }));

        secondary_pc.peer_connection().on_connection_change(Box::new(move |state| {
            let _ = secondary_connection_state_tx.blocking_send(state);
        }));

        let mut lossy_dc = publisher_pc.peer_connection().create_data_channel(LOSSY_DC_LABEL, {
            let mut dc_init = DataChannelInit::default();
            dc_init.ordered = true;
            dc_init.max_retransmits = Some(0);
            dc_init
        })?;

        let mut reliable_dc = publisher_pc.peer_connection().create_data_channel(RELIABLE_DC_LABEL, {
            let mut dc_init = DataChannelInit::default();
            dc_init.ordered = true;
            dc_init
        })?;

        lossy_dc.on_message(Box::new(|data, binary| {
            if let Ok(data) = DataPacket::decode(data) {
                let _ = lossy_data_tx.blocking_send(data);
            } else {
                trace!("lossy_dc - failed to decode DataPacket");
            }
        }));

        reliable_dc.on_message(Box::new(|data, binary| {
            if let Ok(data) = DataPacket::decode(data) {
                let _ = reliable_data_tx.blocking_send(data);
            } else {
                trace!("reliable_dc - failed to decode DataPacket");
            }
        }));

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
        })
    }
}

pub struct RTCEngine {}

/// Initialize the SignalClient & the PeerConnections
//pub async fn connect(url: &str, token: &str) -> Result<RTCEngine, EngineError> {
//}

impl RTCEngine {
    async fn rtc_handle() {
        loop {}
    }
}

#[tokio::test]
async fn test_test() {
    env_logger::init();

    //engine.connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NjQ1OTY4MDYsImlzcyI6IkFQSUNrSG04M01oZ2hQeCIsIm5hbWUiOiJ1c2VyMSIsIm5iZiI6MTY2MDk5NjgwNiwic3ViIjoidXNlcjEiLCJ2aWRlbyI6eyJyb29tIjoibXktZmlyc3Qtcm9vbSIsInJvb21Kb2luIjp0cnVlfX0.SWU_LETMK6ZmFOf38pYjVhpur0o7jJc6u61h8BH7g20").await.unwrap();
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
