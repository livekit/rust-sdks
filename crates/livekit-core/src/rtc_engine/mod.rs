use core::num::flt2dec::Sign;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::sync::{mpsc, Mutex as AsyncMutex};

use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, trace};

use crate::{proto, signal_client};
use livekit_webrtc::data_channel::{DataChannel, DataChannelInit, DataSendError, DataState};
use livekit_webrtc::jsep::{IceCandidate, SdpParseError, SessionDescription};
use livekit_webrtc::media_stream::MediaStream;
use livekit_webrtc::peer_connection::{
    IceConnectionState, PeerConnectionState, RTCOfferAnswerOptions,
};
use livekit_webrtc::peer_connection_factory::RTCConfiguration;
use livekit_webrtc::rtc_error::RTCError;
use livekit_webrtc::rtp_receiver::RtpReceiver;

use crate::proto::data_packet::Value;
use crate::proto::{
    data_packet, signal_request, signal_response, DataPacket, JoinResponse, ParticipantUpdate,
    SignalTarget, TrickleRequest,
};
use crate::rtc_engine::lk_runtime::LKRuntime;
use crate::rtc_engine::pc_transport::PCTransport;
use crate::rtc_engine::rtc_events::{RTCEmitter, RTCEvent, RTCEvents};
use crate::signal_client::{SignalClient, SignalError, SignalEvent, SignalEvents, SignalOptions};

mod lk_runtime;
mod pc_transport;
mod rtc_events;

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

pub(crate) type EngineEmitter = mpsc::Sender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::Receiver<EngineEvent>;
pub(crate) type EngineResult<T> = Result<T, EngineError>;

pub(crate) const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const LOSSY_DC_LABEL: &str = "_lossy";
pub(crate) const RELIABLE_DC_LABEL: &str = "_reliable";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum PCState {
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

#[derive(Debug)]
pub(crate) enum EngineEvent {
    ParticipantUpdate(ParticipantUpdate),
    AddTrack {
        rtp_receiver: RtpReceiver,
        streams: Vec<MediaStream>,
    },
}

#[derive(Debug)]
struct EngineInner {
    has_published: AtomicBool,
    join_response: Mutex<JoinResponse>,
    pc_state: AtomicU8, // Casted to PCState enum

    publisher_pc: AsyncMutex<PCTransport>,
    subscriber_pc: AsyncMutex<PCTransport>,

    // Publisher data channels
    // Used to send data to other participants ( The SFU forward the messages )
    lossy_dc: Mutex<DataChannel>,
    reliable_dc: Mutex<DataChannel>,

    // Subscriber data channels
    // These fields are never used, we just keep a strong reference to them,
    // so we can receive data from other participants
    sub_reliable_dc: Mutex<Option<DataChannel>>,
    sub_lossy_dc: Mutex<Option<DataChannel>>,
}

#[derive(Debug)]
pub struct RTCEngine {
    signal_client: Arc<SignalClient>,
    engine_inner: Arc<EngineInner>,

    #[allow(unused)]
    lk_runtime: Arc<LKRuntime>, // Keep a reference while we're using the RTCEngine
}

impl RTCEngine {
    #[tracing::instrument(skip(url, token))]
    pub(crate) async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<(RTCEngine, EngineEvents)> {
        let mut lk_runtime = None;
        {
            let mut lk_runtime_ref = LK_RUNTIME.lock();
            lk_runtime = lk_runtime_ref.upgrade();

            if lk_runtime.is_none() {
                let new_runtime = Arc::new(LKRuntime::new());
                *lk_runtime_ref = Arc::downgrade(&new_runtime);
                lk_runtime = Some(new_runtime);
            }
        }

        let lk_runtime = lk_runtime.unwrap();
        let (signal_client, mut signal_events) = SignalClient::connect(url, token, options).await?;

        let join_response = signal_client::utils::next_join_response(&mut signal_events).await?;
        debug!("received JoinResponse: {:?}", join_response);

        let (engine_inner, rtc_events) =
            Self::configure_engine(lk_runtime.clone(), join_response.clone())?;
        let engine_inner = Arc::new(engine_inner);
        let signal_client = Arc::new(signal_client);

        let (emitter, events) = mpsc::channel(8);

        tokio::spawn(Self::signal_task(
            signal_client.clone(),
            engine_inner.clone(),
            signal_events,
            emitter.clone(),
        ));

        tokio::spawn(Self::engine_task(
            signal_client.clone(),
            engine_inner.clone(),
            rtc_events,
            emitter.clone(),
        ));

        let rtc_engine = Self {
            signal_client,
            engine_inner,
            lk_runtime,
        };

        if !join_response.subscriber_primary {
            rtc_engine.negotiate_publisher().await?;
        }

        Ok((rtc_engine, events))
    }

    #[tracing::instrument]
    pub async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.ensure_publisher_connected(kind).await?;
        self.data_channel(kind)
            .lock()
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    pub fn join_response(&self) -> JoinResponse {
        self.engine_inner.join_response.lock().clone()
    }

    async fn engine_task(
        signal_client: Arc<SignalClient>,
        engine_inner: Arc<EngineInner>,
        mut rtc_events: RTCEvents,
        emitter: EngineEmitter,
    ) {
        while let Some(event) = rtc_events.recv().await {
            if let Err(err) = Self::handle_rtc(
                event,
                signal_client.clone(),
                engine_inner.clone(),
                emitter.clone(),
            )
            .await
            {
                error!("failed to handle rtc event: {:?}", err);
            }
        }
    }

    async fn signal_task(
        signal_client: Arc<SignalClient>,
        engine_inner: Arc<EngineInner>,
        mut signal_events: SignalEvents,
        emitter: EngineEmitter,
    ) {
        while let Some(signal) = signal_events.recv().await {
            match signal {
                SignalEvent::Open => {}
                SignalEvent::Signal(signal) => {
                    if let Err(err) = Self::handle_signal(
                        signal,
                        signal_client.clone(),
                        engine_inner.clone(),
                        emitter.clone(),
                    )
                    .await
                    {
                        error!("failed to handle signal: {:?}", err);
                    }
                }
                SignalEvent::Close => {
                    // Try reconnect if this isn't expected
                }
            }
        }
    }

    async fn handle_rtc(
        event: RTCEvent,
        signal_client: Arc<SignalClient>,
        engine_inner: Arc<EngineInner>,
        emitter: EngineEmitter,
    ) -> EngineResult<()> {
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

                tokio::spawn(async move {
                    signal_client
                        .send(signal_request::Message::Trickle(TrickleRequest {
                            candidate_init: json,
                            target: target as i32,
                        }))
                        .await;
                });
            }
            RTCEvent::ConnectionChange { state, target } => {
                // Reconnect if we've been disconnected unexpectedly
                let subscriber_primary = engine_inner.join_response.lock().subscriber_primary;
                let is_primary = subscriber_primary && target == SignalTarget::Subscriber;

                if is_primary && state == PeerConnectionState::Disconnected {
                    let old_state = engine_inner
                        .pc_state
                        .swap(PCState::Connected as u8, Ordering::SeqCst);
                    if old_state == PCState::New as u8 {
                        // TODO(theomonnom) Handle disconnect
                    }
                } else if state == PeerConnectionState::Failed {
                    engine_inner
                        .pc_state
                        .store(PCState::Disconnected as u8, Ordering::SeqCst);
                    // TODO(theomonnom) Handle disconnect
                }
            }
            RTCEvent::DataChannel {
                data_channel,
                target,
            } => {
                if target == SignalTarget::Subscriber {
                    if data_channel.label() == RELIABLE_DC_LABEL {
                        *engine_inner.sub_reliable_dc.lock() = Some(data_channel);
                    } else {
                        *engine_inner.sub_lossy_dc.lock() = Some(data_channel);
                    }
                }
            }
            RTCEvent::Offer { offer, target } => {
                if target == SignalTarget::Publisher {
                    // Send the publisher offer to the server
                    tokio::spawn(async move {
                        signal_client
                            .send(signal_request::Message::Offer(proto::SessionDescription {
                                r#type: "offer".to_string(),
                                sdp: offer.to_string(),
                            }))
                            .await;
                    });
                }
            }
            RTCEvent::AddTrack {
                rtp_receiver,
                streams,
                target,
            } => {
                if target == SignalTarget::Subscriber {
                    let _ = emitter.send(EngineEvent::AddTrack {
                        rtp_receiver,
                        streams,
                    });
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

    async fn handle_signal(
        event: signal_response::Message,
        signal_client: Arc<SignalClient>,
        engine_inner: Arc<EngineInner>,
        emitter: EngineEmitter,
    ) -> EngineResult<()> {
        match event {
            signal_response::Message::Answer(answer) => {
                trace!("received answer from the publisher: {:?}", answer);

                let sdp = SessionDescription::from(answer.r#type.parse().unwrap(), &answer.sdp)?;
                engine_inner
                    .publisher_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
            }
            signal_response::Message::Offer(offer) => {
                // Handle the subscriber offer & send an answer to livekit-server
                // We always get an offer from the server when connecting
                trace!("received offer from the publisher: {:?}", offer);
                let sdp = SessionDescription::from(offer.r#type.parse().unwrap(), &offer.sdp)?;

                engine_inner
                    .subscriber_pc
                    .lock()
                    .await
                    .set_remote_description(sdp)
                    .await?;
                let answer = engine_inner
                    .subscriber_pc
                    .lock()
                    .await
                    .peer_connection()
                    .create_answer(RTCOfferAnswerOptions::default())
                    .await?;
                engine_inner
                    .subscriber_pc
                    .lock()
                    .await
                    .peer_connection()
                    .set_local_description(answer.clone())
                    .await?;

                tokio::spawn(async move {
                    signal_client
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

                trace!(
                    "received ice_candidate {:?} - {:?}",
                    SignalTarget::from_i32(trickle.target).unwrap(),
                    ice
                );

                if trickle.target == SignalTarget::Publisher as i32 {
                    engine_inner
                        .publisher_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                } else {
                    engine_inner
                        .subscriber_pc
                        .lock()
                        .await
                        .add_ice_candidate(ice)
                        .await?;
                }
            }
            signal_response::Message::Update(update) => {
                let _ = emitter.send(EngineEvent::ParticipantUpdate(update));
            }
            _ => {}
        }

        Ok(())
    }

    async fn ensure_publisher_connected(&self, kind: data_packet::Kind) -> EngineResult<()> {
        if !self.join_response().subscriber_primary {
            return Ok(());
        }

        let publisher = &self.engine_inner.publisher_pc;
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

    async fn negotiate_publisher(&self) -> EngineResult<()> {
        self.engine_inner
            .has_published
            .store(true, Ordering::SeqCst);
        if let Err(err) = self
            .engine_inner
            .publisher_pc
            .lock()
            .await
            .negotiate()
            .await
        {
            error!("failed to negotiate the publisher: {:?}", err);
            Err(err)?
        } else {
            Ok(())
        }
    }

    fn configure_engine(
        lk_runtime: Arc<LKRuntime>,
        join_response: JoinResponse,
    ) -> EngineResult<(EngineInner, RTCEvents)> {
        let (rtc_emitter, events) = mpsc::unbounded_channel();
        let rtc_config = RTCConfiguration::from(join_response.clone());

        let mut publisher_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
        );

        let mut subscriber_pc = PCTransport::new(
            lk_runtime
                .pc_factory
                .create_peer_connection(rtc_config.clone())?,
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

        publisher_pc
            .peer_connection()
            .on_ice_candidate(rtc_events::on_ice_candidate(
                SignalTarget::Publisher,
                rtc_emitter.clone(),
            ));
        subscriber_pc
            .peer_connection()
            .on_ice_candidate(rtc_events::on_ice_candidate(
                SignalTarget::Subscriber,
                rtc_emitter.clone(),
            ));

        publisher_pc.on_offer(rtc_events::on_offer(
            SignalTarget::Publisher,
            rtc_emitter.clone(),
        ));
        subscriber_pc.on_offer(rtc_events::on_offer(
            SignalTarget::Subscriber,
            rtc_emitter.clone(),
        ));

        publisher_pc
            .peer_connection()
            .on_data_channel(rtc_events::on_data_channel(
                SignalTarget::Publisher,
                rtc_emitter.clone(),
            ));
        subscriber_pc
            .peer_connection()
            .on_data_channel(rtc_events::on_data_channel(
                SignalTarget::Subscriber,
                rtc_emitter.clone(),
            ));

        publisher_pc
            .peer_connection()
            .on_add_track(rtc_events::on_add_track(
                SignalTarget::Publisher,
                rtc_emitter.clone(),
            ));
        subscriber_pc
            .peer_connection()
            .on_add_track(rtc_events::on_add_track(
                SignalTarget::Subscriber,
                rtc_emitter.clone(),
            ));

        publisher_pc
            .peer_connection()
            .on_connection_change(rtc_events::on_connection_change(
                SignalTarget::Publisher,
                rtc_emitter.clone(),
            ));
        subscriber_pc
            .peer_connection()
            .on_connection_change(rtc_events::on_connection_change(
                SignalTarget::Subscriber,
                rtc_emitter.clone(),
            ));

        lossy_dc.on_message(rtc_events::on_message(rtc_emitter.clone()));
        reliable_dc.on_message(rtc_events::on_message(rtc_emitter.clone()));

        Ok((
            EngineInner {
                has_published: AtomicBool::new(false),
                join_response: Mutex::new(join_response),
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

    fn data_channel(&self, kind: data_packet::Kind) -> &Mutex<DataChannel> {
        if kind == data_packet::Kind::Reliable {
            &self.engine_inner.reliable_dc
        } else {
            &self.engine_inner.lossy_dc
        }
    }
}
