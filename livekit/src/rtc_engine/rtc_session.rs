// Copyright 2023 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{rtc_events, EngineError, EngineResult, SimulateScenario};
use crate::id::ParticipantSid;
use crate::options::TrackPublishOptions;
use crate::prelude::TrackKind;
use crate::room::DisconnectReason;
use crate::rtc_engine::lk_runtime::LkRuntime;
use crate::rtc_engine::peer_transport::PeerTransport;
use crate::rtc_engine::rtc_events::{RtcEvent, RtcEvents};
use crate::track::LocalTrack;
use crate::DataPacketKind;
use livekit_api::signal_client::{SignalClient, SignalEvent, SignalEvents, SignalOptions};
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use parking_lot::Mutex;
use prost::Message;
use proto::debouncer::{self, Debouncer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;

pub const ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const TRACK_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
pub const LOSSY_DC_LABEL: &str = "_lossy";
pub const RELIABLE_DC_LABEL: &str = "_reliable";
pub const PUBLISHER_NEGOTIATION_FREQUENCY: Duration = Duration::from_millis(150);

pub type SessionEmitter = mpsc::UnboundedSender<SessionEvent>;
pub type SessionEvents = mpsc::UnboundedReceiver<SessionEvent>;

#[derive(Debug)]
pub enum SessionEvent {
    ParticipantUpdate {
        updates: Vec<proto::ParticipantInfo>,
    },
    Data {
        participant_sid: ParticipantSid,
        payload: Vec<u8>,
        kind: DataPacketKind,
    },
    MediaTrack {
        track: MediaStreamTrack,
        stream: MediaStream,
        receiver: RtpReceiver,
    },
    SpeakersChanged {
        speakers: Vec<proto::SpeakerInfo>,
    },
    ConnectionQuality {
        updates: Vec<proto::ConnectionQualityInfo>,
    },
    // TODO(theomonnom): Move entirely the reconnection logic on mod.rs
    Close {
        source: String,
        reason: DisconnectReason,
        can_reconnect: bool,
        full_reconnect: bool,
        retry_now: bool,
    },
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    New,
    Connected,
    Disconnected,
    Reconnecting,
    Closed,
}

impl TryFrom<u8> for PeerState {
    type Error = &'static str;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::New),
            1 => Ok(Self::Connected),
            2 => Ok(Self::Disconnected),
            3 => Ok(Self::Reconnecting),
            4 => Ok(Self::Closed),
            _ => Err("invalid PeerState"),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IceCandidateJson {
    pub sdp_mid: String,
    pub sdp_m_line_index: i32,
    pub candidate: String,
}

/// Fields shared with rtc_task and signal_task
struct SessionInner {
    signal_client: Arc<SignalClient>,
    pc_state: AtomicU8, // PcState
    has_published: AtomicBool,

    publisher_pc: PeerTransport,
    subscriber_pc: PeerTransport,

    pending_tracks: Mutex<HashMap<String, oneshot::Sender<proto::TrackInfo>>>,

    // Publisher data channels
    // used to send data to other participants (The SFU forwards the messages)
    lossy_dc: DataChannel,
    reliable_dc: DataChannel,

    // Keep a strong reference to the subscriber datachannels,
    // so we can receive data from other participants
    subscriber_dc: Mutex<Vec<DataChannel>>,

    closed: AtomicBool,
    emitter: SessionEmitter,

    negotiation_debouncer: Mutex<Option<Debouncer>>,
}

impl Debug for SessionInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionInner")
            .field("pc_state", &self.pc_state)
            .field("has_published", &self.has_published)
            .field("closed", &self.closed)
            .finish()
    }
}

/// This struct holds a WebRTC session
/// The session changes at every reconnection
///
/// RTCSession is also responsable for the signaling and the negotation
#[derive(Debug)]
pub struct RtcSession {
    inner: Arc<SessionInner>,
    close_tx: watch::Sender<bool>, // false = is_running
    signal_task: JoinHandle<()>,
    rtc_task: JoinHandle<()>,
}

impl RtcSession {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<(Self, SessionEvents)> {
        let (session_emitter, session_events) = mpsc::unbounded_channel();

        let (signal_client, join_response, signal_events) =
            SignalClient::connect(url, token, options).await?;
        let signal_client = Arc::new(signal_client);
        log::debug!("received JoinResponse: {:?}", join_response);

        let (rtc_emitter, rtc_events) = mpsc::unbounded_channel();
        let rtc_config = make_rtc_config_join(join_response);

        let lk_runtime = LkRuntime::instance();
        let mut publisher_pc = PeerTransport::new(
            lk_runtime
                .pc_factory()
                .create_peer_connection(rtc_config.clone())?,
            proto::SignalTarget::Publisher,
        );

        let mut subscriber_pc = PeerTransport::new(
            lk_runtime
                .pc_factory()
                .create_peer_connection(rtc_config.clone())?,
            proto::SignalTarget::Subscriber,
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

        // Forward events received inside the signaling thread to our rtc channel
        rtc_events::forward_pc_events(&mut publisher_pc, rtc_emitter.clone());
        rtc_events::forward_pc_events(&mut subscriber_pc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut lossy_dc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut reliable_dc, rtc_emitter.clone());

        let (close_tx, close_rx) = watch::channel(false);
        let inner = Arc::new(SessionInner {
            pc_state: AtomicU8::new(PeerState::New as u8),
            has_published: Default::default(),
            signal_client,
            publisher_pc,
            subscriber_pc,
            pending_tracks: Default::default(),
            lossy_dc,
            reliable_dc,
            subscriber_dc: Default::default(),
            closed: Default::default(),
            emitter: session_emitter,
            negotiation_debouncer: Default::default(),
        });

        // Start session tasks
        let signal_task = tokio::spawn(inner.clone().signal_task(signal_events, close_rx.clone()));
        let rtc_task = tokio::spawn(inner.clone().rtc_session_task(rtc_events, close_rx.clone()));

        let session = Self {
            inner: inner.clone(),
            close_tx,
            signal_task,
            rtc_task,
        };

        Ok((session, session_events))
    }

    pub async fn add_track(&self, req: proto::AddTrackRequest) -> EngineResult<proto::TrackInfo> {
        self.inner.add_track(req).await
    }

    pub async fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        self.inner.remove_track(sender).await
    }

    pub async fn create_sender(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
        encodings: Vec<RtpEncodingParameters>,
    ) -> EngineResult<RtpTransceiver> {
        self.inner.create_sender(track, options, encodings).await
    }

    pub fn publisher_negotiation_needed(&self) {
        self.inner.publisher_negotiation_needed()
    }

    /// Close the PeerConnections and the SignalClient
    pub async fn close(self) {
        // Close the tasks
        self.inner.close().await;
        let _ = self.close_tx.send(true);
        let _ = self.rtc_task.await;
        let _ = self.signal_task.await;
    }

    pub async fn publish_data(
        &self,
        data: &proto::DataPacket,
        kind: DataPacketKind,
    ) -> Result<(), EngineError> {
        self.inner.publish_data(data, kind).await
    }

    pub async fn restart(&self) -> EngineResult<()> {
        self.inner.restart_session().await
    }

    pub async fn wait_pc_connection(&self) -> EngineResult<()> {
        self.inner.wait_pc_connection().await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) {
        self.inner.simulate_scenario(scenario).await
    }

    #[allow(dead_code)]
    pub fn state(&self) -> PeerState {
        self.inner
            .pc_state
            .load(Ordering::SeqCst)
            .try_into()
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn publisher(&self) -> &PeerTransport {
        &self.inner.publisher_pc
    }

    pub fn subscriber(&self) -> &PeerTransport {
        &self.inner.subscriber_pc
    }

    pub fn signal_client(&self) -> &Arc<SignalClient> {
        &self.inner.signal_client
    }

    #[allow(dead_code)]
    pub fn data_channel(&self, kind: DataPacketKind) -> &DataChannel {
        &self.inner.data_channel(kind)
    }

    pub fn data_channels_info(&self) -> Vec<proto::DataChannelInfo> {
        self.inner.data_channels_info()
    }
}

impl SessionInner {
    async fn rtc_session_task(
        self: Arc<Self>,
        mut rtc_events: RtcEvents,
        mut close_rx: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                res = rtc_events.recv() => {
                    if let Some(event) = res {
                        if let Err(err) = self.on_rtc_event(event).await {
                            log::error!("failed to handle rtc event: {:?}", err);
                        }
                    }                },
                 _ = close_rx.changed() => {
                    log::trace!("closing rtc_session_task");
                    break;
                }
            }
        }
    }

    async fn signal_task(
        self: Arc<Self>,
        mut signal_events: SignalEvents,
        mut close_rx: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                res = signal_events.recv() => {
                    if let Some(signal) = res {
                        match signal {
                            SignalEvent::Open => {}
                            SignalEvent::Signal(signal) => {
                                if let Err(err) = self.on_signal_event(signal).await {
                                    log::error!("failed to handle signal: {:?}", err);
                                }
                            }
                            SignalEvent::Close => {
                                self.on_session_disconnected(
                                    "signalclient closed",
                                    DisconnectReason::UnknownReason,
                                    true,
                                    false,
                                    false
                                );
                            }
                        }
                    }
                },
                _ = close_rx.changed() => {
                    log::trace!("closing signal_task");
                    break;
                }
            }
        }
    }

    async fn on_signal_event(&self, event: proto::signal_response::Message) -> EngineResult<()> {
        match event {
            proto::signal_response::Message::Answer(answer) => {
                log::debug!("received publisher answer: {:?}", answer);
                let answer =
                    SessionDescription::parse(&answer.sdp, answer.r#type.parse().unwrap())?;
                self.publisher_pc.set_remote_description(answer).await?;
            }
            proto::signal_response::Message::Offer(offer) => {
                log::debug!("received subscriber offer: {:?}", offer);
                let offer = SessionDescription::parse(&offer.sdp, offer.r#type.parse().unwrap())?;
                let answer = self
                    .subscriber_pc
                    .create_anwser(offer, AnswerOptions::default())
                    .await?;

                self.signal_client
                    .send(proto::signal_request::Message::Answer(
                        proto::SessionDescription {
                            r#type: "answer".to_string(),
                            sdp: answer.to_string(),
                        },
                    ))
                    .await;
            }
            proto::signal_response::Message::Trickle(trickle) => {
                let target = trickle.target();
                let ice_candidate = {
                    let json = serde_json::from_str::<IceCandidateJson>(&trickle.candidate_init)?;
                    IceCandidate::parse(&json.sdp_mid, json.sdp_m_line_index, &json.candidate)?
                };

                log::debug!("received ice_candidate {:?} {:?}", target, ice_candidate);

                if target == proto::SignalTarget::Publisher {
                    self.publisher_pc.add_ice_candidate(ice_candidate).await?;
                } else {
                    self.subscriber_pc.add_ice_candidate(ice_candidate).await?;
                }
            }
            proto::signal_response::Message::Leave(leave) => {
                log::debug!("received leave request: {:?}", leave);
                self.on_session_disconnected(
                    "server request to leave",
                    leave.reason(),
                    leave.can_reconnect,
                    true,
                    true,
                );
            }
            proto::signal_response::Message::Update(update) => {
                let _ = self.emitter.send(SessionEvent::ParticipantUpdate {
                    updates: update.participants,
                });
            }
            proto::signal_response::Message::SpeakersChanged(speaker) => {
                let _ = self.emitter.send(SessionEvent::SpeakersChanged {
                    speakers: speaker.speakers,
                });
            }
            proto::signal_response::Message::ConnectionQuality(quality) => {
                let _ = self.emitter.send(SessionEvent::ConnectionQuality {
                    updates: quality.updates,
                });
            }
            proto::signal_response::Message::TrackPublished(publish_res) => {
                let mut pending_tracks = self.pending_tracks.lock();
                if let Some(tx) = pending_tracks.remove(&publish_res.cid) {
                    let _ = tx.send(publish_res.track.unwrap());
                }
            }
            proto::signal_response::Message::Reconnect(reconnect) => {
                log::debug!("received reconnect request: {:?}", reconnect);
                let rtc_config = make_rtc_config_reconnect(reconnect);
                self.publisher_pc
                    .peer_connection()
                    .set_configuration(rtc_config.clone())?;
                self.subscriber_pc
                    .peer_connection()
                    .set_configuration(rtc_config)?;
            }

            _ => {}
        }

        Ok(())
    }

    async fn on_rtc_event(&self, event: RtcEvent) -> EngineResult<()> {
        match event {
            RtcEvent::IceCandidate {
                ice_candidate,
                target,
            } => {
                self.signal_client
                    .send(proto::signal_request::Message::Trickle(
                        proto::TrickleRequest {
                            candidate_init: serde_json::to_string(&IceCandidateJson {
                                sdp_mid: ice_candidate.sdp_mid(),
                                sdp_m_line_index: ice_candidate.sdp_mline_index(),
                                candidate: ice_candidate.candidate(),
                            })?,
                            target: target as i32,
                        },
                    ))
                    .await;
            }
            RtcEvent::ConnectionChange { state, target } => {
                log::debug!("connection change, {:?} {:?}", state, target);

                // The subscriber is always the primary peer connection
                if target == proto::SignalTarget::Subscriber
                    && state == PeerConnectionState::Connected
                {
                    let old_state = self
                        .pc_state
                        .swap(PeerState::Connected as u8, Ordering::SeqCst);
                    if old_state == PeerState::New as u8 {
                        let _ = self.emitter.send(SessionEvent::Connected);
                    }
                } else if state == PeerConnectionState::Failed {
                    log::error!("{:?} pc state failed", target);
                    self.pc_state
                        .store(PeerState::Disconnected as u8, Ordering::SeqCst);

                    self.on_session_disconnected(
                        "pc_state failed",
                        DisconnectReason::UnknownReason,
                        true,
                        false,
                        false,
                    );
                }
            }
            RtcEvent::DataChannel {
                data_channel,
                target: _,
            } => {
                self.subscriber_dc.lock().push(data_channel);
            }
            RtcEvent::Offer { offer, target: _ } => {
                // Send the publisher offer to the server
                log::debug!("sending publisher offer: {:?}", offer);
                self.signal_client
                    .send(proto::signal_request::Message::Offer(
                        proto::SessionDescription {
                            r#type: "offer".to_string(),
                            sdp: offer.to_string(),
                        },
                    ))
                    .await;
            }
            RtcEvent::Track {
                receiver,
                mut streams,
                track,
                transceiver: _,
                target: _,
            } => {
                if !streams.is_empty() {
                    let _ = self.emitter.send(SessionEvent::MediaTrack {
                        stream: streams.remove(0),
                        track,
                        receiver,
                    });
                } else {
                    log::warn!("Track event with no streams");
                }
            }
            RtcEvent::Data { data, binary } => {
                if !binary {
                    Err(EngineError::Internal(
                        "text messages aren't supported".to_string(),
                    ))?;
                }

                let data = proto::DataPacket::decode(&*data)?;
                match data.value.as_ref().unwrap() {
                    proto::data_packet::Value::User(user) => {
                        let _ = self.emitter.send(SessionEvent::Data {
                            kind: data.kind().into(),
                            participant_sid: user.participant_sid.clone().try_into().unwrap(),
                            payload: user.payload.clone(),
                        });
                    }
                    proto::data_packet::Value::Speaker(_) => {}
                }
            }
        }

        Ok(())
    }

    async fn add_track(&self, req: proto::AddTrackRequest) -> EngineResult<proto::TrackInfo> {
        let (tx, rx) = oneshot::channel();
        let cid = req.cid.clone();
        {
            let mut pendings_tracks = self.pending_tracks.lock();
            if pendings_tracks.contains_key(&req.cid) {
                Err(EngineError::Internal("track already published".to_string()))?;
            }

            pendings_tracks.insert(cid.clone(), tx);
        }

        self.signal_client
            .send(proto::signal_request::Message::AddTrack(req))
            .await;

        // Wait the result from the server (TrackInfo)
        tokio::select! {
            Ok(info) = rx => Ok(info),
            _ = sleep(TRACK_PUBLISH_TIMEOUT) => {
                self.pending_tracks.lock().remove(&cid);
                Err(EngineError::Internal("track publication timed out, no response received from the server".to_string()))
            },
            else => {
                Err(EngineError::Internal(
                    "track publication cancelled".to_string(),
                ))
            }
        }
    }

    async fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        if let Some(track) = sender.track() {
            let mut pending_tracks = self.pending_tracks.lock();
            pending_tracks.remove(&track.id());
        }

        self.publisher_pc.peer_connection().remove_track(sender)?;

        Ok(())
    }

    async fn create_sender(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
        encodings: Vec<RtpEncodingParameters>,
    ) -> EngineResult<RtpTransceiver> {
        let init = RtpTransceiverInit {
            direction: RtpTransceiverDirection::SendOnly,
            stream_ids: Default::default(),
            send_encodings: encodings,
        };

        let transceiver = self
            .publisher_pc
            .peer_connection()
            .add_transceiver(track.rtc_track(), init)?;

        if track.kind() == TrackKind::Video {
            let capabilities = LkRuntime::instance()
                .pc_factory()
                .get_rtp_sender_capabilities(match track.kind() {
                    TrackKind::Video => MediaType::Video,
                    TrackKind::Audio => MediaType::Audio,
                });

            let mut matched = Vec::new();
            let mut partial_matched = Vec::new();
            let mut unmatched = Vec::new();

            for codec in capabilities.codecs {
                let mime_type = codec.mime_type.to_lowercase();
                if mime_type == format!("video/{}", options.video_codec.as_str()) {
                    if let Some(sdp_fmtp_line) = codec.sdp_fmtp_line.as_ref() {
                        // for h264 codecs that have sdpFmtpLine available, use only if the
                        // profile-level-id is 42e01f for cross-browser compatibility
                        if sdp_fmtp_line.contains("profile-level-id=42e01f") {
                            matched.push(codec);
                            continue;
                        }
                    }
                    partial_matched.push(codec);
                } else {
                    unmatched.push(codec);
                }
            }

            matched.append(&mut partial_matched);
            matched.append(&mut unmatched);

            transceiver.set_codec_preferences(matched)?;
        }

        Ok(transceiver)
    }

    /// Called when the SignalClient or one of the PeerConnection has lost the connection
    /// The RTCEngine may try a reconnect.
    fn on_session_disconnected(
        &self,
        source: &str,
        reason: DisconnectReason,
        can_reconnect: bool,
        retry_now: bool,
        full_reconnect: bool,
    ) {
        let _ = self.emitter.send(SessionEvent::Close {
            source: source.to_owned(),
            reason,
            can_reconnect,
            retry_now,
            full_reconnect,
        });
    }

    async fn close(&self) {
        let _ = self
            .signal_client
            .send(proto::signal_request::Message::Leave(proto::LeaveRequest {
                can_reconnect: false,
                reason: DisconnectReason::ClientInitiated as i32,
            }))
            .await;

        self.closed.store(true, Ordering::Release);
        self.signal_client.close().await;
        self.publisher_pc.close();
        self.subscriber_pc.close();
    }

    async fn simulate_scenario(&self, scenario: SimulateScenario) {
        match scenario {
            SimulateScenario::SignalReconnect => {
                self.signal_client.close().await;
            }
            SimulateScenario::Speaker => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(proto::simulate_scenario::Scenario::SpeakerUpdate(3)),
                        },
                    ))
                    .await;
            }
            SimulateScenario::NodeFailure => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(proto::simulate_scenario::Scenario::NodeFailure(true)),
                        },
                    ))
                    .await;
            }
            SimulateScenario::ServerLeave => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(proto::simulate_scenario::Scenario::ServerLeave(true)),
                        },
                    ))
                    .await;
            }
            SimulateScenario::Migration => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(proto::simulate_scenario::Scenario::Migration(true)),
                        },
                    ))
                    .await;
            }
            SimulateScenario::ForceTcp => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(
                                proto::simulate_scenario::Scenario::SwitchCandidateProtocol(
                                    proto::CandidateProtocol::Tcp as i32,
                                ),
                            ),
                        },
                    ))
                    .await;
            }
            SimulateScenario::ForceTls => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(
                        proto::SimulateScenario {
                            scenario: Some(
                                proto::simulate_scenario::Scenario::SwitchCandidateProtocol(
                                    proto::CandidateProtocol::Tls as i32,
                                ),
                            ),
                        },
                    ))
                    .await;
            }
        }
    }

    async fn publish_data(
        self: &Arc<Self>,
        data: &proto::DataPacket,
        kind: DataPacketKind,
    ) -> Result<(), EngineError> {
        self.ensure_publisher_connected(kind).await?;
        self.data_channel(kind)
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    /// Try to restart the session by doing an ICE Restart (The SignalClient is also restarted)
    /// This reconnection if more seemless compared to the full reconnection implemented in ['RTCEngine']
    async fn restart_session(&self) -> EngineResult<()> {
        self.signal_client.restart().await?;
        self.subscriber_pc.prepare_ice_restart().await;

        if self.has_published.load(Ordering::Acquire) {
            self.publisher_pc
                .create_and_send_offer(OfferOptions {
                    ice_restart: true,
                    ..Default::default()
                })
                .await?;
        }

        self.signal_client.flush_queue().await;
        Ok(())
    }

    /// Wait for PeerState to become PeerState::Connected
    /// Timeout after ['MAX_ICE_CONNECT_TIMEOUT']
    async fn wait_pc_connection(&self) -> EngineResult<()> {
        let wait_connected = async move {
            while self.pc_state.load(Ordering::Acquire) != PeerState::Connected as u8 {
                if self.closed.load(Ordering::Acquire) {
                    return Err(EngineError::Connection("closed".to_string()));
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            Ok(())
        };

        tokio::select! {
            res = wait_connected => res,
            _ = sleep(ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("wait_pc_connection timed out".to_string());
                Err(err)
            }
        }
    }

    /// Start publisher negotiation
    fn publisher_negotiation_needed(self: &Arc<Self>) {
        self.has_published.store(true, Ordering::Relaxed);

        let mut debouncer = self.negotiation_debouncer.lock();

        // call() returns an error if the debouncer has finished
        if debouncer.is_none() || debouncer.as_ref().unwrap().call().is_err() {
            let session = self.clone();

            *debouncer = Some(debouncer::debounce(
                PUBLISHER_NEGOTIATION_FREQUENCY,
                async move {
                    if let Err(err) = session
                        .publisher_pc
                        .create_and_send_offer(OfferOptions::default())
                        .await
                    {
                        log::error!("failed to negotiate the publisher: {:?}", err);
                    }
                },
            ));
        }
    }

    /// Ensure the Publisher PC is connected, if not, start the negotiation
    /// This is required when sending data to the server
    async fn ensure_publisher_connected(
        self: &Arc<Self>,
        kind: DataPacketKind,
    ) -> EngineResult<()> {
        if !self.publisher_pc.is_connected()
            && self.publisher_pc.peer_connection().ice_connection_state()
                != IceConnectionState::Checking
        {
            self.publisher_negotiation_needed();
        }

        let dc = self.data_channel(kind);
        if dc.state() == DataState::Open {
            return Ok(());
        }

        // Wait until the PeerConnection is connected
        let wait_connected = async {
            while !self.publisher_pc.is_connected() || dc.state() != DataState::Open {
                if self.closed.load(Ordering::Acquire) {
                    return Err(EngineError::Connection("closed".to_string()));
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            Ok(())
        };

        tokio::select! {
            res = wait_connected => res,
            _ = sleep(ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("could not establish publisher connection: timeout".to_string());
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    fn data_channel(&self, kind: DataPacketKind) -> &DataChannel {
        if kind == DataPacketKind::Reliable {
            &self.reliable_dc
        } else {
            &self.lossy_dc
        }
    }

    /// Used to send client states to the server on migration
    fn data_channels_info(&self) -> Vec<proto::DataChannelInfo> {
        let mut vec = Vec::with_capacity(4);

        vec.push(proto::DataChannelInfo {
            label: self.lossy_dc.label(),
            id: self.lossy_dc.id() as u32,
            target: proto::SignalTarget::Publisher as i32,
        });

        vec.push(proto::DataChannelInfo {
            label: self.reliable_dc.label(),
            id: self.reliable_dc.id() as u32,
            target: proto::SignalTarget::Publisher as i32,
        });

        for dc in self.subscriber_dc.lock().iter() {
            vec.push(proto::DataChannelInfo {
                label: dc.label(),
                id: dc.id() as u32,
                target: proto::SignalTarget::Subscriber as i32,
            });
        }

        vec
    }
}

macro_rules! make_rtc_config {
    ($fncname:ident, $proto:ty) => {
        fn $fncname(value: $proto) -> RtcConfiguration {
            RtcConfiguration {
                ice_servers: {
                    let mut servers = Vec::with_capacity(value.ice_servers.len());
                    for ice_server in value.ice_servers.clone() {
                        servers.push(IceServer {
                            urls: ice_server.urls,
                            username: ice_server.username,
                            password: ice_server.credential,
                        })
                    }
                    servers
                },
                continual_gathering_policy: ContinualGatheringPolicy::GatherContinually,
                ice_transport_type: IceTransportsType::All,
            }
        }
    };
}

make_rtc_config!(make_rtc_config_join, proto::JoinResponse);
make_rtc_config!(make_rtc_config_reconnect, proto::ReconnectResponse);
