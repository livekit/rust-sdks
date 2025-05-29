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

use std::{
    collections::{HashMap, VecDeque},
    convert::TryInto,
    fmt::Debug,
    ops::Not,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use libwebrtc::{prelude::*, stats::RtcStats};
use livekit_api::signal_client::{SignalClient, SignalEvent, SignalEvents};
use livekit_protocol as proto;
use livekit_runtime::{sleep, JoinHandle};
use parking_lot::Mutex;
use prost::Message;
use proto::{
    debouncer::{self, Debouncer},
    SignalTarget,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch, Notify};

use super::{rtc_events, EngineError, EngineOptions, EngineResult, SimulateScenario};
use crate::{id::ParticipantIdentity, ChatMessage, TranscriptionSegment};
use crate::{
    id::ParticipantSid,
    options::TrackPublishOptions,
    prelude::TrackKind,
    room::DisconnectReason,
    rtc_engine::{
        lk_runtime::LkRuntime,
        peer_transport::PeerTransport,
        rtc_events::{RtcEvent, RtcEvents},
    },
    track::LocalTrack,
    DataPacketKind,
};

pub const ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const TRACK_PUBLISH_TIMEOUT: Duration = Duration::from_secs(10);
pub const LOSSY_DC_LABEL: &str = "_lossy";
pub const RELIABLE_DC_LABEL: &str = "_reliable";
pub const PUBLISHER_NEGOTIATION_FREQUENCY: Duration = Duration::from_millis(150);
pub const INITIAL_BUFFERED_AMOUNT_LOW_THRESHOLD: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
enum NegotiationState {
    Idle,
    InProgress,
    PendingRetry,
}

struct NegotiationQueue {
    state: Arc<Mutex<NegotiationState>>,
    waker: Arc<Notify>,
    task_running: AtomicBool,
    waiting_for_answer: AtomicBool,
}

impl NegotiationQueue {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NegotiationState::Idle)),
            waker: Arc::new(Notify::new()),
            task_running: AtomicBool::new(false),
            waiting_for_answer: AtomicBool::new(false),
        }
    }
}

pub type SessionEmitter = mpsc::UnboundedSender<SessionEvent>;
pub type SessionEvents = mpsc::UnboundedReceiver<SessionEvent>;

#[derive(Debug, Clone)]
pub struct SessionStats {
    pub publisher_stats: Vec<RtcStats>,
    pub subscriber_stats: Vec<RtcStats>,
}

#[derive(Debug)]
pub enum SessionEvent {
    ParticipantUpdate {
        updates: Vec<proto::ParticipantInfo>,
    },
    Data {
        // None when the data comes from the ServerSDK (So no real participant)
        participant_sid: Option<ParticipantSid>,
        participant_identity: Option<ParticipantIdentity>,
        payload: Vec<u8>,
        topic: Option<String>,
        kind: DataPacketKind,
    },
    ChatMessage {
        participant_identity: ParticipantIdentity,
        message: ChatMessage,
    },
    Transcription {
        participant_identity: ParticipantIdentity,
        track_sid: String,
        segments: Vec<TranscriptionSegment>,
    },
    SipDTMF {
        // None when the data comes from the ServerSDK (So no real participant)
        participant_identity: Option<ParticipantIdentity>,
        code: u32,
        digit: Option<String>,
    },
    RpcRequest {
        caller_identity: Option<ParticipantIdentity>,
        request_id: String,
        method: String,
        payload: String,
        response_timeout: Duration,
        version: u32,
    },
    RpcResponse {
        request_id: String,
        payload: Option<String>,
        error: Option<proto::RpcError>,
    },
    RpcAck {
        request_id: String,
    },
    MediaTrack {
        track: MediaStreamTrack,
        stream: MediaStream,
        transceiver: RtpTransceiver,
    },
    SpeakersChanged {
        speakers: Vec<proto::SpeakerInfo>,
    },
    ConnectionQuality {
        updates: Vec<proto::ConnectionQualityInfo>,
    },
    RoomUpdate {
        room: proto::Room,
    },
    LocalTrackSubscribed {
        track_sid: String,
    },
    Close {
        source: String,
        reason: DisconnectReason,
        action: proto::leave_request::Action,
        retry_now: bool,
    },
    DataStreamHeader {
        header: proto::data_stream::Header,
        participant_identity: String,
    },
    DataStreamChunk {
        chunk: proto::data_stream::Chunk,
        participant_identity: String,
    },
    DataStreamTrailer {
        trailer: proto::data_stream::Trailer,
        participant_identity: String,
    },
    DataChannelBufferedAmountLowThresholdChanged {
        kind: DataPacketKind,
        threshold: u64,
    },
    RefreshToken {
        url: String,
        token: String,
    },
}

#[derive(Debug)]
enum DataChannelEvent {
    PublishData(proto::DataPacket, DataPacketKind, oneshot::Sender<Result<(), EngineError>>),
    BufferedAmountChange(u64, DataPacketKind),
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
    has_published: AtomicBool,
    fast_publish: AtomicBool,

    publisher_pc: PeerTransport,
    subscriber_pc: PeerTransport,

    pending_tracks: Mutex<HashMap<String, oneshot::Sender<proto::TrackInfo>>>,

    // Publisher data channels
    // used to send data to other participants (The SFU forwards the messages)
    lossy_dc: DataChannel,
    lossy_dc_buffered_amount_low_threshold: AtomicU64,
    reliable_dc: DataChannel,
    reliable_dc_buffered_amount_low_threshold: AtomicU64,
    dc_emitter: mpsc::UnboundedSender<DataChannelEvent>,

    // Keep a strong reference to the subscriber datachannels,
    // so we can receive data from other participants
    sub_lossy_dc: Mutex<Option<DataChannel>>,
    sub_reliable_dc: Mutex<Option<DataChannel>>,

    closed: AtomicBool,
    emitter: SessionEmitter,

    options: EngineOptions,
    negotiation_debouncer: Mutex<Option<Debouncer>>,
    negotiation_queue: NegotiationQueue,

    pending_requests: Mutex<HashMap<u32, oneshot::Sender<proto::RequestResponse>>>,
}

/// This struct holds a WebRTC session
/// The session changes at every reconnection
///
/// RTCSession is also responsable for the signaling and the negotation
pub struct RtcSession {
    inner: Arc<SessionInner>,
    handle: Mutex<Option<SessionHandle>>,
}

impl Debug for RtcSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcSession").finish()
    }
}

struct SessionHandle {
    close_tx: watch::Sender<bool>, // false = is_running
    signal_task: JoinHandle<()>,
    rtc_task: JoinHandle<()>,
    dc_task: JoinHandle<()>,
}

impl RtcSession {
    pub async fn connect(
        url: &str,
        token: &str,
        options: EngineOptions,
    ) -> EngineResult<(Self, proto::JoinResponse, SessionEvents)> {
        let (emitter, session_events) = mpsc::unbounded_channel();

        let (signal_client, join_response, signal_events) =
            SignalClient::connect(url, token, options.signal_options.clone()).await?;
        let signal_client = Arc::new(signal_client);
        log::debug!("received JoinResponse: {:?}", join_response);

        let (rtc_emitter, rtc_events) = mpsc::unbounded_channel();
        let rtc_config = make_rtc_config_join(join_response.clone(), options.rtc_config.clone());

        let (dc_emitter, dc_events) = mpsc::unbounded_channel();

        let lk_runtime = LkRuntime::instance();
        let mut publisher_pc = PeerTransport::new(
            lk_runtime.pc_factory().create_peer_connection(rtc_config.clone())?,
            proto::SignalTarget::Publisher,
        );

        let mut subscriber_pc = PeerTransport::new(
            lk_runtime.pc_factory().create_peer_connection(rtc_config)?,
            proto::SignalTarget::Subscriber,
        );

        let mut lossy_dc = publisher_pc.peer_connection().create_data_channel(
            LOSSY_DC_LABEL,
            DataChannelInit {
                ordered: false,
                max_retransmits: Some(0),
                ..DataChannelInit::default()
            },
        )?;

        let mut reliable_dc = publisher_pc.peer_connection().create_data_channel(
            RELIABLE_DC_LABEL,
            DataChannelInit { ordered: true, ..DataChannelInit::default() },
        )?;

        // Forward events received inside the signaling thread to our rtc channel
        rtc_events::forward_pc_events(&mut publisher_pc, rtc_emitter.clone());
        rtc_events::forward_pc_events(&mut subscriber_pc, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut lossy_dc, DataPacketKind::Lossy, rtc_emitter.clone());
        rtc_events::forward_dc_events(&mut reliable_dc, DataPacketKind::Reliable, rtc_emitter);

        let (close_tx, close_rx) = watch::channel(false);

        let inner = Arc::new(SessionInner {
            has_published: Default::default(),
            fast_publish: AtomicBool::new(join_response.fast_publish),
            signal_client,
            publisher_pc,
            subscriber_pc,
            pending_tracks: Default::default(),
            lossy_dc,
            lossy_dc_buffered_amount_low_threshold: AtomicU64::new(
                INITIAL_BUFFERED_AMOUNT_LOW_THRESHOLD,
            ),
            reliable_dc,
            reliable_dc_buffered_amount_low_threshold: AtomicU64::new(
                INITIAL_BUFFERED_AMOUNT_LOW_THRESHOLD,
            ),
            dc_emitter,
            sub_lossy_dc: Mutex::new(None),
            sub_reliable_dc: Mutex::new(None),
            closed: Default::default(),
            emitter,
            options,
            negotiation_debouncer: Default::default(),
            negotiation_queue: NegotiationQueue::new(),
            pending_requests: Default::default(),
        });

        // Start session tasks
        let signal_task =
            livekit_runtime::spawn(inner.clone().signal_task(signal_events, close_rx.clone()));
        let rtc_task =
            livekit_runtime::spawn(inner.clone().rtc_session_task(rtc_events, close_rx.clone()));
        let dc_task = livekit_runtime::spawn(inner.clone().data_channel_task(dc_events, close_rx));

        let handle = Mutex::new(Some(SessionHandle { close_tx, signal_task, rtc_task, dc_task }));

        Ok((Self { inner, handle }, join_response, session_events))
    }

    pub fn has_published(&self) -> bool {
        self.inner.has_published.load(Ordering::Acquire)
    }

    pub fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        self.inner.remove_track(sender)
    }

    pub fn publisher_negotiation_needed(&self) {
        self.inner.publisher_negotiation_needed()
    }

    pub async fn add_track(&self, req: proto::AddTrackRequest) -> EngineResult<proto::TrackInfo> {
        self.inner.add_track(req).await
    }

    pub async fn mute_track(&self, req: proto::MuteTrackRequest) -> EngineResult<()> {
        self.inner.mute_track(req).await
    }

    pub async fn create_sender(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
        encodings: Vec<RtpEncodingParameters>,
    ) -> EngineResult<RtpTransceiver> {
        self.inner.create_sender(track, options, encodings).await
    }

    /// Close the PeerConnections and the SignalClient
    pub async fn close(&self) {
        // Close the tasks
        let handle = self.handle.lock().take();
        if let Some(handle) = handle {
            let _ = handle.close_tx.send(true);
            let _ = handle.rtc_task.await;
            let _ = handle.signal_task.await;
            let _ = handle.dc_task.await;
        }

        // Close the PeerConnections after the task
        // So if a sensitive operation is running, we can wait for it
        self.inner.close().await;
    }

    pub async fn publish_data(
        &self,
        data: proto::DataPacket,
        kind: DataPacketKind,
    ) -> Result<(), EngineError> {
        self.inner.publish_data(data, kind).await
    }

    pub async fn restart(&self) -> EngineResult<proto::ReconnectResponse> {
        self.inner.restart().await
    }

    pub async fn restart_publisher(&self) -> EngineResult<()> {
        self.inner.restart_publisher().await
    }

    pub async fn wait_pc_connection(&self) -> EngineResult<()> {
        self.inner.wait_pc_connection().await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.simulate_scenario(scenario).await
    }

    pub async fn get_stats(&self) -> EngineResult<SessionStats> {
        let publisher_stats = self.inner.publisher_pc.peer_connection().get_stats().await?;

        let subscriber_stats = self.inner.subscriber_pc.peer_connection().get_stats().await?;

        Ok(SessionStats { publisher_stats, subscriber_stats })
    }

    pub fn publisher(&self) -> &PeerTransport {
        &self.inner.publisher_pc
    }

    pub fn subscriber(&self) -> &PeerTransport {
        &self.inner.subscriber_pc
    }

    pub fn signal_client(&self) -> &Arc<SignalClient> {
        &self.inner.signal_client
    }

    pub fn data_channel(&self, target: SignalTarget, kind: DataPacketKind) -> Option<DataChannel> {
        self.inner.data_channel(target, kind)
    }

    pub fn data_channel_buffered_amount_low_threshold(&self, kind: DataPacketKind) -> u64 {
        match kind {
            DataPacketKind::Lossy => {
                self.inner.lossy_dc_buffered_amount_low_threshold.load(Ordering::Relaxed)
            }
            DataPacketKind::Reliable => {
                self.inner.reliable_dc_buffered_amount_low_threshold.load(Ordering::Relaxed)
            }
        }
    }

    pub fn set_data_channel_buffered_amount_low_threshold(
        &self,
        threshold: u64,
        kind: DataPacketKind,
    ) {
        match kind {
            DataPacketKind::Lossy => self
                .inner
                .lossy_dc_buffered_amount_low_threshold
                .store(threshold, Ordering::Relaxed),
            DataPacketKind::Reliable => self
                .inner
                .reliable_dc_buffered_amount_low_threshold
                .store(threshold, Ordering::Relaxed),
        }
        let _ = self
            .inner
            .emitter
            .send(SessionEvent::DataChannelBufferedAmountLowThresholdChanged { kind, threshold });
    }

    pub async fn get_response(&self, request_id: u32) -> proto::RequestResponse {
        self.inner.get_response(request_id).await
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
                Some(event) = rtc_events.recv() => {
                    let debug = format!("{:?}", event);
                    let inner = self.clone();
                    let (tx, rx) = oneshot::channel();
                    let task = livekit_runtime::spawn(async move {
                        if let Err(err) = inner.on_rtc_event(event).await {
                            log::error!("failed to handle rtc event: {:?}", err);
                        }
                        let _ = tx.send(());
                    });

                    // Monitor sync/async blockings
                    tokio::select! {
                        _ = rx => {},
                        _ = livekit_runtime::sleep(Duration::from_secs(10)) => {
                            log::error!("rtc_event is taking too much time: {}", debug);
                        }
                    }

                    task.await;
                },
                _ = close_rx.changed() => {
                    break;
                }
            }
        }

        log::debug!("rtc_session_task closed");
    }

    async fn signal_task(
        self: Arc<Self>,
        mut signal_events: SignalEvents,
        mut close_rx: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                Some(signal) = signal_events.recv() => {
                    match signal {
                        SignalEvent::Message(signal) => {
                            let debug = format!("{:?}", signal);
                            let inner = self.clone();
                            let (tx, rx) = oneshot::channel();
                            let task = livekit_runtime::spawn(async move {
                                if let Err(err) = inner.on_signal_event(*signal).await {
                                    log::error!("failed to handle signal: {:?}", err);
                                }
                                let _ = tx.send(());
                            });

                            // Monitor sync/async blockings
                            tokio::select! {
                                _ = rx => {},
                                _ = livekit_runtime::sleep(Duration::from_secs(10)) => {
                                    log::error!("signal_event taking too much time: {}", debug);
                                }
                            }

                            task.await;
                        }
                        SignalEvent::Close(reason) => {
                            if !self.closed.load(Ordering::Acquire) {
                                // SignalClient has been closed unexpectedly
                                self.on_session_disconnected(
                                    format!("signal client closed: {:?}", reason).as_str(),
                                    DisconnectReason::UnknownReason,
                                    proto::leave_request::Action::Resume,
                                    false,
                                );
                            }
                        }
                    }
                },
                _ = close_rx.changed() => {
                    break;
                }
            }
        }

        log::debug!("closing signal_task");
    }

    async fn data_channel_task(
        self: Arc<Self>,
        mut dc_events: mpsc::UnboundedReceiver<DataChannelEvent>,
        mut close_rx: watch::Receiver<bool>,
    ) {
        let mut lossy_buffered_amount = 0;
        let mut reliable_buffered_amount = 0;
        let mut lossy_queue = VecDeque::new();
        let mut reliable_queue = VecDeque::new();

        loop {
            tokio::select! {
                event = dc_events.recv() => {
                    let Some(event) = event else {
                        // tx closed
                        break;
                    };

                    match event {
                        DataChannelEvent::PublishData(packet, kind, tx) => {
                            let data = packet.encode_to_vec();
                            match kind {
                                DataPacketKind::Lossy => {
                                    lossy_queue.push_back((data, kind, tx));
                                    let threshold = self.lossy_dc_buffered_amount_low_threshold.load(Ordering::Relaxed);
                                    self._send_until_threshold(threshold, &mut lossy_buffered_amount, &mut lossy_queue);
                                }
                                DataPacketKind::Reliable => {
                                    reliable_queue.push_back((data, kind, tx));
                                    let threshold = self.reliable_dc_buffered_amount_low_threshold.load(Ordering::Relaxed);
                                    self._send_until_threshold(threshold, &mut reliable_buffered_amount, &mut reliable_queue);
                                }
                            }
                        }
                        DataChannelEvent::BufferedAmountChange(sent, kind) => {
                            match kind {
                                DataPacketKind::Lossy => {
                                    if lossy_buffered_amount < sent {
                                        // I believe never reach here but adding logs just in case
                                        log::error!("unexpected buffer size detected: lossy_buffered_amount={}, sent={}", lossy_buffered_amount, sent);
                                        lossy_buffered_amount = 0;
                                    } else {
                                        lossy_buffered_amount -= sent;
                                    }
                                    let threshold = self.lossy_dc_buffered_amount_low_threshold.load(Ordering::Relaxed);
                                    self._send_until_threshold(threshold, &mut lossy_buffered_amount, &mut lossy_queue);
                                }
                                DataPacketKind::Reliable => {
                                    if reliable_buffered_amount < sent {
                                        log::error!("unexpected buffer size detected: reliable_buffered_amount={}, sent={}", reliable_buffered_amount, sent);
                                        reliable_buffered_amount = 0;
                                    } else {
                                        reliable_buffered_amount -= sent;
                                    }
                                    let threshold = self.reliable_dc_buffered_amount_low_threshold.load(Ordering::Relaxed);
                                    self._send_until_threshold(threshold, &mut reliable_buffered_amount, &mut reliable_queue);
                                }
                            }
                        }
                    }
                },

                _ = close_rx.changed() => {
                    break;
                },
            }
        }

        log::debug!("closing data_channel_task");
    }

    fn _send_until_threshold(
        self: &Arc<Self>,
        threshold: u64,
        buffered_amount: &mut u64,
        queue: &mut VecDeque<(Vec<u8>, DataPacketKind, oneshot::Sender<Result<(), EngineError>>)>,
    ) {
        while *buffered_amount <= threshold {
            let Some((data, kind, tx)) = queue.pop_front() else {
                break;
            };

            *buffered_amount += data.len() as u64;
            let result = self
                .data_channel(SignalTarget::Publisher, kind)
                .unwrap()
                .send(&data, true)
                .map_err(|err| {
                    EngineError::Internal(format!("failed to send data packet: {:?}", err).into())
                });

            let _ = tx.send(result);
        }
    }

    async fn on_signal_event(&self, event: proto::signal_response::Message) -> EngineResult<()> {
        match event {
            proto::signal_response::Message::Answer(answer) => {
                log::debug!("received publisher answer: {:?}", answer);
                let answer =
                    SessionDescription::parse(&answer.sdp, answer.r#type.parse().unwrap()).unwrap(); // Unwrap is ok, the server shouldn't give us an invalid sdp
                self.publisher_pc.set_remote_description(answer).await?;

                if self.fast_publish.load(Ordering::Acquire) {
                    if self.negotiation_queue.waiting_for_answer.swap(false, Ordering::AcqRel) {
                        log::debug!("answer received, notifying negotiation loop");
                        self.negotiation_queue.waker.notify_one();
                    }
                }
            }
            proto::signal_response::Message::Offer(offer) => {
                log::debug!("received subscriber offer: {:?}", offer);
                let offer =
                    SessionDescription::parse(&offer.sdp, offer.r#type.parse().unwrap()).unwrap();
                let answer =
                    self.subscriber_pc.create_anwser(offer, AnswerOptions::default()).await?;

                self.signal_client
                    .send(proto::signal_request::Message::Answer(proto::SessionDescription {
                        r#type: "answer".to_string(),
                        sdp: answer.to_string(),
                    }))
                    .await;
            }
            proto::signal_response::Message::Trickle(trickle) => {
                let target = trickle.target();
                let ice_candidate = {
                    let json =
                        serde_json::from_str::<IceCandidateJson>(&trickle.candidate_init).unwrap();
                    IceCandidate::parse(&json.sdp_mid, json.sdp_m_line_index, &json.candidate)
                        .unwrap()
                };

                log::debug!("remote ice_candidate {:?} {:?}", ice_candidate, target);

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
                    leave.action(),
                    true,
                );
            }
            proto::signal_response::Message::Update(update) => {
                let _ = self
                    .emitter
                    .send(SessionEvent::ParticipantUpdate { updates: update.participants });
            }
            proto::signal_response::Message::SpeakersChanged(speaker) => {
                let _ =
                    self.emitter.send(SessionEvent::SpeakersChanged { speakers: speaker.speakers });
            }
            proto::signal_response::Message::ConnectionQuality(quality) => {
                let _ =
                    self.emitter.send(SessionEvent::ConnectionQuality { updates: quality.updates });
            }
            proto::signal_response::Message::TrackPublished(publish_res) => {
                let mut pending_tracks = self.pending_tracks.lock();
                if let Some(tx) = pending_tracks.remove(&publish_res.cid) {
                    let _ = tx.send(publish_res.track.unwrap());
                }
            }
            proto::signal_response::Message::RoomUpdate(room_update) => {
                let _ =
                    self.emitter.send(SessionEvent::RoomUpdate { room: room_update.room.unwrap() });
            }
            proto::signal_response::Message::TrackSubscribed(track_subscribed) => {
                let _ = self.emitter.send(SessionEvent::LocalTrackSubscribed {
                    track_sid: track_subscribed.track_sid,
                });
            }
            proto::signal_response::Message::RequestResponse(request_response) => {
                let mut pending_requests = self.pending_requests.lock();
                if let Some(tx) = pending_requests.remove(&request_response.request_id) {
                    let _ = tx.send(request_response);
                }
            }
            proto::signal_response::Message::RefreshToken(ref token) => {
                let url = self.signal_client.url();
                let _ = self.emitter.send(SessionEvent::RefreshToken { url, token: token.clone() });
            }
            _ => {}
        }

        Ok(())
    }

    async fn on_rtc_event(&self, event: RtcEvent) -> EngineResult<()> {
        match event {
            RtcEvent::IceCandidate { ice_candidate, target } => {
                log::debug!("local ice_candidate {:?} {:?}", ice_candidate, target);
                self.signal_client
                    .send(proto::signal_request::Message::Trickle(proto::TrickleRequest {
                        candidate_init: serde_json::to_string(&IceCandidateJson {
                            sdp_mid: ice_candidate.sdp_mid(),
                            sdp_m_line_index: ice_candidate.sdp_mline_index(),
                            candidate: ice_candidate.candidate(),
                        })
                        .unwrap(),
                        target: target as i32,
                        ..Default::default()
                    }))
                    .await;
            }
            RtcEvent::ConnectionChange { state, target } => {
                log::debug!("connection change, {:?} {:?}", state, target);

                if state == PeerConnectionState::Failed {
                    log::error!("{:?} pc state failed", target);
                    self.on_session_disconnected(
                        "pc_state failed",
                        DisconnectReason::UnknownReason,
                        proto::leave_request::Action::Resume,
                        false,
                    );
                }
            }
            RtcEvent::DataChannel { data_channel, target } => {
                log::debug!("received data channel: {:?} {:?}", data_channel, target);
                if target == SignalTarget::Subscriber {
                    if data_channel.label() == LOSSY_DC_LABEL {
                        self.sub_lossy_dc.lock().replace(data_channel);
                    } else if data_channel.label() == RELIABLE_DC_LABEL {
                        self.sub_reliable_dc.lock().replace(data_channel);
                    }
                }
            }
            RtcEvent::Offer { offer, target: _ } => {
                // Send the publisher offer to the server
                log::debug!("sending publisher offer: {:?}", offer);
                self.signal_client
                    .send(proto::signal_request::Message::Offer(proto::SessionDescription {
                        r#type: "offer".to_string(),
                        sdp: offer.to_string(),
                    }))
                    .await;
            }
            RtcEvent::Track { mut streams, track, transceiver, target: _ } => {
                if !streams.is_empty() {
                    let _ = self.emitter.send(SessionEvent::MediaTrack {
                        stream: streams.remove(0),
                        track,
                        transceiver,
                    });
                } else {
                    log::warn!("Track event with no streams");
                }
            }
            RtcEvent::Data { data, binary } => {
                if !binary {
                    Err(EngineError::Internal("text messages aren't supported".into()))?;
                }

                let data = proto::DataPacket::decode(&*data).unwrap();
                if let Some(packet) = data.value.as_ref() {
                    match packet {
                        proto::data_packet::Value::User(user) => {
                            let participant_sid = user
                                .participant_sid
                                .is_empty()
                                .not()
                                .then_some(user.participant_sid.clone());

                            let participant_identity = if !data.participant_identity.is_empty() {
                                Some(data.participant_identity.clone())
                            } else if !user.participant_identity.is_empty() {
                                Some(user.participant_identity.clone())
                            } else {
                                None
                            };

                            let _ = self.emitter.send(SessionEvent::Data {
                                kind: data.kind().into(),
                                participant_sid: participant_sid.map(|s| s.try_into().unwrap()),
                                participant_identity: participant_identity
                                    .map(|s| s.try_into().unwrap()),
                                payload: user.payload.clone(),
                                topic: user.topic.clone(),
                            });
                        }
                        proto::data_packet::Value::SipDtmf(dtmf) => {
                            let participant_identity = data
                                .participant_identity
                                .is_empty()
                                .not()
                                .then_some(data.participant_identity.clone());
                            let digit = dtmf.digit.is_empty().not().then_some(dtmf.digit.clone());

                            let _ = self.emitter.send(SessionEvent::SipDTMF {
                                participant_identity: participant_identity
                                    .map(|s| s.try_into().unwrap()),
                                digit: digit.map(|s| s.try_into().unwrap()),
                                code: dtmf.code,
                            });
                        }
                        proto::data_packet::Value::Speaker(_) => {}
                        proto::data_packet::Value::Transcription(transcription) => {
                            let track_sid = transcription.track_id.clone();
                            // let segments = transcription.segments.clone();
                            let segments = transcription
                                .segments
                                .iter()
                                .map(|s| TranscriptionSegment {
                                    id: s.id.clone(),
                                    start_time: s.start_time,
                                    end_time: s.end_time,
                                    text: s.text.clone(),
                                    language: s.language.clone(),
                                    r#final: s.r#final,
                                })
                                .collect();
                            let participant_identity: ParticipantIdentity =
                                transcription.transcribed_participant_identity.clone().into();
                            let _ = self.emitter.send(SessionEvent::Transcription {
                                participant_identity,
                                track_sid,
                                segments,
                            });
                        }
                        proto::data_packet::Value::RpcRequest(rpc_request) => {
                            let caller_identity = data
                                .participant_identity
                                .is_empty()
                                .not()
                                .then_some(data.participant_identity.clone())
                                .map(|s| s.try_into().unwrap());
                            let _ = self.emitter.send(SessionEvent::RpcRequest {
                                caller_identity,
                                request_id: rpc_request.id.clone(),
                                method: rpc_request.method.clone(),
                                payload: rpc_request.payload.clone(),
                                response_timeout: Duration::from_millis(
                                    rpc_request.response_timeout_ms as u64,
                                ),
                                version: rpc_request.version,
                            });
                        }
                        proto::data_packet::Value::RpcResponse(rpc_response) => {
                            let _ = self.emitter.send(SessionEvent::RpcResponse {
                                request_id: rpc_response.request_id.clone(),
                                payload: rpc_response.value.as_ref().and_then(|v| match v {
                                    proto::rpc_response::Value::Payload(payload) => {
                                        Some(payload.clone())
                                    }
                                    _ => None,
                                }),
                                error: rpc_response.value.as_ref().and_then(|v| match v {
                                    proto::rpc_response::Value::Error(error) => Some(error.clone()),
                                    _ => None,
                                }),
                            });
                        }
                        proto::data_packet::Value::RpcAck(rpc_ack) => {
                            let _ = self.emitter.send(SessionEvent::RpcAck {
                                request_id: rpc_ack.request_id.clone(),
                            });
                        }
                        proto::data_packet::Value::ChatMessage(message) => {
                            let _ = self.emitter.send(SessionEvent::ChatMessage {
                                participant_identity: ParticipantIdentity(
                                    data.participant_identity,
                                ),
                                message: ChatMessage::from(message.clone()),
                            });
                        }
                        proto::data_packet::Value::StreamHeader(message) => {
                            let _ = self.emitter.send(SessionEvent::DataStreamHeader {
                                header: message.clone(),
                                participant_identity: data.participant_identity.clone(),
                            });
                        }
                        proto::data_packet::Value::StreamChunk(message) => {
                            let _ = self.emitter.send(SessionEvent::DataStreamChunk {
                                chunk: message.clone(),
                                participant_identity: data.participant_identity.clone(),
                            });
                        }
                        proto::data_packet::Value::StreamTrailer(message) => {
                            let _ = self.emitter.send(SessionEvent::DataStreamTrailer {
                                trailer: message.clone(),
                                participant_identity: data.participant_identity.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            RtcEvent::DataChannelBufferedAmountChange { sent, amount: _, kind } => {
                if let Err(err) =
                    self.dc_emitter.send(DataChannelEvent::BufferedAmountChange(sent, kind))
                {
                    log::error!("failed to send dc_event buffer_amount_change: {:?}", err);
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
                Err(EngineError::Internal("track already published".into()))?;
            }

            pendings_tracks.insert(cid.clone(), tx);
        }

        self.signal_client.send(proto::signal_request::Message::AddTrack(req)).await;

        // Wait the result from the server (TrackInfo)
        tokio::select! {
            Ok(info) = rx => Ok(info),
            _ = sleep(TRACK_PUBLISH_TIMEOUT) => {
                self.pending_tracks.lock().remove(&cid);
                Err(EngineError::Internal("track publication timed out, no response received from the server".into()))
            },
            else => {
                Err(EngineError::Internal("track publication cancelled".into()))
            }
        }
    }

    fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        if let Some(track) = sender.track() {
            let mut pending_tracks = self.pending_tracks.lock();
            pending_tracks.remove(&track.id());
        }

        self.publisher_pc.peer_connection().remove_track(sender)?;

        Ok(())
    }

    async fn mute_track(&self, req: proto::MuteTrackRequest) -> EngineResult<()> {
        self.signal_client.send(proto::signal_request::Message::Mute(req)).await;

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

        let transceiver =
            self.publisher_pc.peer_connection().add_transceiver(track.rtc_track(), init)?;

        if track.kind() == TrackKind::Video {
            let capabilities = LkRuntime::instance().pc_factory().get_rtp_sender_capabilities(
                match track.kind() {
                    TrackKind::Video => MediaType::Video,
                    TrackKind::Audio => MediaType::Audio,
                },
            );

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
        action: proto::leave_request::Action,
        retry_now: bool,
    ) {
        let _ = self.emitter.send(SessionEvent::Close {
            source: source.to_owned(),
            reason,
            action,
            retry_now,
        });
    }

    async fn close(&self) {
        self.closed.store(true, Ordering::Release);

        self.signal_client
            .send(proto::signal_request::Message::Leave(proto::LeaveRequest {
                action: proto::leave_request::Action::Disconnect.into(),
                reason: DisconnectReason::ClientInitiated as i32,
                ..Default::default()
            }))
            .await;

        self.signal_client.close().await;
        self.publisher_pc.close();
        self.subscriber_pc.close();
    }

    async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        let simulate_leave = || {
            self.on_signal_event(proto::signal_response::Message::Leave(proto::LeaveRequest {
                action: proto::leave_request::Action::Reconnect.into(),
                reason: DisconnectReason::ClientInitiated as i32,
                ..Default::default()
            }))
        };

        match scenario {
            SimulateScenario::SignalReconnect => {
                self.signal_client.close().await;
            }
            SimulateScenario::Speaker => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(proto::simulate_scenario::Scenario::SpeakerUpdate(3)),
                    }))
                    .await;
            }
            SimulateScenario::NodeFailure => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(proto::simulate_scenario::Scenario::NodeFailure(true)),
                    }))
                    .await;
            }
            SimulateScenario::ServerLeave => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(proto::simulate_scenario::Scenario::ServerLeave(true)),
                    }))
                    .await;
            }
            SimulateScenario::Migration => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(proto::simulate_scenario::Scenario::Migration(true)),
                    }))
                    .await;
            }
            SimulateScenario::ForceTcp => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(
                            proto::simulate_scenario::Scenario::SwitchCandidateProtocol(
                                proto::CandidateProtocol::Tcp as i32,
                            ),
                        ),
                    }))
                    .await;

                simulate_leave().await?
            }
            SimulateScenario::ForceTls => {
                self.signal_client
                    .send(proto::signal_request::Message::Simulate(proto::SimulateScenario {
                        scenario: Some(
                            proto::simulate_scenario::Scenario::SwitchCandidateProtocol(
                                proto::CandidateProtocol::Tls as i32,
                            ),
                        ),
                    }))
                    .await;

                simulate_leave().await?
            }
        }
        Ok(())
    }

    async fn publish_data(
        self: &Arc<Self>,
        data: proto::DataPacket,
        kind: DataPacketKind,
    ) -> Result<(), EngineError> {
        self.ensure_publisher_connected(kind).await?;

        let (tx, rx) = oneshot::channel();
        if let Err(err) = self.dc_emitter.send(DataChannelEvent::PublishData(data, kind, tx)) {
            return Err(EngineError::Internal(
                format!("failed to push data into queue: {:?}", err).into(),
            ));
        };
        rx.await.map_err(|e| {
            EngineError::Internal(format!("failed to receive data from dc_task: {:?}", e).into())
        })?
    }

    /// This reconnection if more seemless compared to the full reconnection implemented in
    /// ['RTCEngine']
    async fn restart(&self) -> EngineResult<proto::ReconnectResponse> {
        let reconnect_response = self.signal_client.restart().await?;
        log::debug!("received reconnect response: {:?}", reconnect_response);

        let rtc_config =
            make_rtc_config_reconnect(reconnect_response.clone(), self.options.rtc_config.clone());
        self.publisher_pc.peer_connection().set_configuration(rtc_config.clone())?;
        self.subscriber_pc.peer_connection().set_configuration(rtc_config)?;

        Ok(reconnect_response)
    }

    async fn restart_publisher(&self) -> EngineResult<()> {
        if self.has_published.load(Ordering::Acquire) {
            self.publisher_pc
                .create_and_send_offer(OfferOptions { ice_restart: true, ..Default::default() })
                .await?;
        }
        Ok(())
    }

    /// Timeout after ['MAX_ICE_CONNECT_TIMEOUT']
    async fn wait_pc_connection(&self) -> EngineResult<()> {
        let wait_connected = async move {
            while !self.subscriber_pc.is_connected()
                || (self.has_published.load(Ordering::Acquire) && !self.publisher_pc.is_connected())
            {
                if self.closed.load(Ordering::Acquire) {
                    return Err(EngineError::Connection("closed".into()));
                }

                livekit_runtime::sleep(Duration::from_millis(50)).await;
            }

            Ok(())
        };

        tokio::select! {
            res = wait_connected => res,
            _ = sleep(ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("wait_pc_connection timed out".into());
                Err(err)
            }
        }
    }

    /// Start publisher negotiation
    fn publisher_negotiation_needed(self: &Arc<Self>) {
        let fast_publish = self.fast_publish.load(Ordering::Acquire);
        self.has_published.store(true, Ordering::Release);

        log::debug!("publisher_negotiation_needed: fast_publish={}", fast_publish);

        if fast_publish {
            if self.negotiation_queue.waiting_for_answer.load(Ordering::Acquire) {
                log::debug!("already waiting for answer, marking for retry");
                let mut state = self.negotiation_queue.state.lock();
                *state = NegotiationState::PendingRetry;
                return;
            }
            self.queue_negotiation();
        } else {
            self.debounce_negotiation();
        }
    }

    fn queue_negotiation(self: &Arc<Self>) {
        let mut state = self.negotiation_queue.state.lock();

        match *state {
            NegotiationState::Idle => {
                if self.negotiation_queue.task_running.swap(true, Ordering::AcqRel) {
                    log::debug!("queue_negotiation: task already running, marking for retry");
                    *state = NegotiationState::PendingRetry;
                    return;
                }

                log::debug!("queue_negotiation: starting new negotiation");
                *state = NegotiationState::InProgress;
                drop(state);

                let session = self.clone();
                livekit_runtime::spawn(async move {
                    session.execute_negotiation_with_retry().await;
                    session.negotiation_queue.task_running.store(false, Ordering::Release);
                });
            }
            NegotiationState::InProgress => {
                log::debug!("queue_negotiation: marking for retry");
                *state = NegotiationState::PendingRetry;
            }
            NegotiationState::PendingRetry => {
                log::debug!("queue_negotiation: already pending retry");
            }
        }
    }

    async fn execute_negotiation_with_retry(self: &Arc<Self>) {
        loop {
            log::debug!("negotiating the publisher (fast mode)");

            self.negotiation_queue.waiting_for_answer.store(true, Ordering::Release);

            if let Err(err) = self.publisher_pc.create_and_send_offer(OfferOptions::default()).await
            {
                log::error!("failed to negotiate the publisher: {:?}", err);
                self.negotiation_queue.waiting_for_answer.store(false, Ordering::Release);
            } else {
                log::debug!("offer sent, waiting for answer...");

                let timeout = tokio::time::sleep(Duration::from_secs(10));
                tokio::pin!(timeout);

                tokio::select! {
                    _ = self.negotiation_queue.waker.notified() => {
                        log::debug!("answer received successfully");
                    }
                    _ = &mut timeout => {
                        log::debug!("timeout waiting for answer");
                        self.negotiation_queue.waiting_for_answer.store(false, Ordering::Release);
                    }
                }
            }

            let mut state = self.negotiation_queue.state.lock();
            match *state {
                NegotiationState::PendingRetry => {
                    log::debug!("retrying negotiation");
                    *state = NegotiationState::InProgress;
                    drop(state);
                    continue;
                }
                _ => {
                    log::debug!("negotiation completed");
                    *state = NegotiationState::Idle;
                    break;
                }
            }
        }
    }

    fn debounce_negotiation(self: &Arc<Self>) {
        let mut debouncer = self.negotiation_debouncer.lock();

        // call() returns an error if the debouncer has finished
        if debouncer.is_none() || debouncer.as_ref().unwrap().call().is_err() {
            let session = self.clone();

            *debouncer = Some(debouncer::debounce(PUBLISHER_NEGOTIATION_FREQUENCY, async move {
                log::debug!("negotiating the publisher (debounced)");
                if let Err(err) =
                    session.publisher_pc.create_and_send_offer(OfferOptions::default()).await
                {
                    log::error!("failed to negotiate the publisher: {:?}", err);
                }
            }));
        }
    }

    /// Ensure the Publisher PC is connected, if not, start the negotiation
    /// This is required when sending data to the server
    async fn ensure_publisher_connected(
        self: &Arc<Self>,
        kind: DataPacketKind,
    ) -> EngineResult<()> {
        if !self.has_published.load(Ordering::Acquire) {
            // The publisher has never been connected, start the negotiation
            // If the connection fails, the reconnection logic will be triggered
            self.publisher_negotiation_needed();
        }

        let dc = self.data_channel(SignalTarget::Publisher, kind).unwrap();
        if dc.state() == DataChannelState::Open {
            return Ok(());
        }

        // Wait until the PeerConnection is connected
        let wait_connected = async {
            while !self.publisher_pc.is_connected() || dc.state() != DataChannelState::Open {
                if self.closed.load(Ordering::Acquire) {
                    return Err(EngineError::Connection("closed".into()));
                }

                livekit_runtime::sleep(Duration::from_millis(50)).await;
            }

            Ok(())
        };

        tokio::select! {
            res = wait_connected => res,
            _ = sleep(ICE_CONNECT_TIMEOUT) => {
                let err = EngineError::Connection("could not establish publisher connection: timeout".into());
                log::error!("{}", err);
                Err(err)
            }
        }
    }

    fn data_channel(&self, target: SignalTarget, kind: DataPacketKind) -> Option<DataChannel> {
        if target == SignalTarget::Publisher {
            if kind == DataPacketKind::Reliable {
                Some(self.reliable_dc.clone())
            } else {
                Some(self.lossy_dc.clone())
            }
        } else if target == SignalTarget::Subscriber {
            if kind == DataPacketKind::Reliable {
                self.sub_reliable_dc.lock().clone()
            } else {
                self.sub_lossy_dc.lock().clone()
            }
        } else {
            unreachable!()
        }
    }

    async fn get_response(&self, request_id: u32) -> proto::RequestResponse {
        let (tx, rx) = oneshot::channel();
        self.pending_requests.lock().insert(request_id, tx);
        rx.await.unwrap()
    }
}

macro_rules! make_rtc_config {
    ($fncname:ident, $proto:ty) => {
        fn $fncname(value: $proto, mut config: RtcConfiguration) -> RtcConfiguration {
            if config.ice_servers.is_empty() {
                for ice_server in value.ice_servers.clone() {
                    config.ice_servers.push(IceServer {
                        urls: ice_server.urls,
                        username: ice_server.username,
                        password: ice_server.credential,
                    })
                }
            }

            if let Some(client_configuration) = value.client_configuration {
                if client_configuration.force_relay() == proto::ClientConfigSetting::Enabled {
                    config.ice_transport_type = IceTransportsType::Relay;
                }
            }

            config
        }
    };
}

make_rtc_config!(make_rtc_config_join, proto::JoinResponse);
make_rtc_config!(make_rtc_config_reconnect, proto::ReconnectResponse);
