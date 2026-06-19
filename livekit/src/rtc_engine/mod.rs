// Copyright 2025 LiveKit, Inc.
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

use libwebrtc::prelude::*;
use livekit_api::signal_client::{SignalError, SignalOptions};
use livekit_datatrack::backend as dt;
use livekit_protocol as proto;
use livekit_runtime::JoinHandle;
use parking_lot::{RwLock, RwLockReadGuard};
use std::{
    borrow::Cow,
    fmt::Debug,
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::sync::{
    mpsc, oneshot, Notify, RwLock as AsyncRwLock, RwLockReadGuard as AsyncRwLockReadGuard,
};

pub use self::rtc_session::{SessionStats, INITIAL_BUFFERED_AMOUNT_LOW_THRESHOLD};
use crate::prelude::ParticipantIdentity;
use crate::{
    id::ParticipantSid,
    options::TrackPublishOptions,
    prelude::LocalTrack,
    room::DisconnectReason,
    rtc_engine::{
        lk_runtime::LkRuntime,
        rtc_session::{RtcSession, SessionEvent, SessionEvents},
    },
    DataPacketKind,
};
use crate::{ChatMessage, E2eeManager, TranscriptionSegment};

mod dc_sender;
pub mod lk_runtime;
mod peer_transport;
mod reconnect_strategy;
mod rtc_events;
mod rtc_session;

// Re-exported to preserve the public `rtc_engine::RECONNECT_*` paths.
pub use reconnect_strategy::{
    RECONNECT_ATTEMPTS, RECONNECT_BACKOFF_MULTIPLIER, RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY,
};

pub(crate) type EngineEmitter = mpsc::UnboundedSender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::UnboundedReceiver<EngineEvent>;
pub(crate) type EngineResult<T> = Result<T, EngineError>;

/// Settling delay before checking PeerConnection state on the resume path.
///
/// Lets a freshly issued ICE-restart offer/answer round-trip take effect when the
/// underlying PC was still in `Connected` at the moment we started the reconnect
/// (e.g. signal-only failure). Without this, the resume can return success
/// immediately and the next failure detector then trips the engine into a real
/// disconnect.
///
/// Only applied to the resume path. Full reconnect builds brand-new PCs which
/// don't suffer from the "looks-Connected-but-isn't" race.
pub const PC_RECONNECT_SETTLE_DELAY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SimulateScenario {
    /// Closes the signal channel locally; the engine attempts a Resume.
    SignalReconnect,
    Speaker,
    NodeFailure,
    ServerLeave,
    Migration,
    ForceTcp,
    ForceTls,
    /// Client-driven full reconnect: forces the next reconnect to be a full
    /// reconnect (new RtcSession, republish required) and triggers it locally,
    /// without relying on the server. Mirrors client-sdk-js's `full-reconnect`.
    FullReconnect,
    /// Asks the server to drop the signalling connection during the next resume,
    /// then triggers a resume locally. The resume cannot complete, so the engine
    /// escalates to a full reconnect — exercising the resume→full escalation
    /// path. Mirrors client-sdk-js's `disconnect-signal-on-resume`.
    DisconnectSignalOnResume,
}

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("signal failure: {0}")]
    Signal(#[from] SignalError),
    #[error("internal webrtc failure")]
    Rtc(#[from] RtcError),
    #[error("connection error: {0}")]
    Connection(Cow<'static, str>), // Connectivity issues (Failed to connect/reconnect)
    #[error("internal error: {0}")]
    Internal(Cow<'static, str>), // Unexpected error, generally we can't recover
}

#[derive(Default, Debug, Clone)]
pub struct EngineOptions {
    pub rtc_config: RtcConfiguration,
    pub signal_options: SignalOptions,
    pub join_retries: u32,
    /// Enable single peer connection mode
    pub single_peer_connection: bool,
}

#[derive(Debug)]
pub enum EngineEvent {
    ParticipantUpdate {
        updates: Vec<proto::ParticipantInfo>,
    },
    MediaTrack {
        track: MediaStreamTrack,
        stream: MediaStream,
        transceiver: RtpTransceiver,
    },
    Data {
        participant_sid: Option<ParticipantSid>,
        participant_identity: Option<ParticipantIdentity>,
        payload: Vec<u8>,
        topic: Option<String>,
        kind: DataPacketKind,
        encryption_type: proto::encryption::Type,
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
    SpeakersChanged {
        speakers: Vec<proto::SpeakerInfo>,
    },
    ConnectionQuality {
        updates: Vec<proto::ConnectionQualityInfo>,
    },
    RoomUpdate {
        room: proto::Room,
    },
    RoomMoved {
        moved: proto::RoomMovedResponse,
    },
    /// The following events are used to notify the room about the reconnection state
    /// Since the room needs to also sync state in a good timing with the server.
    /// We synchronize the state with a one-shot channel.
    Resuming(oneshot::Sender<()>),
    Resumed(oneshot::Sender<()>),
    SignalResumed {
        reconnect_response: proto::ReconnectResponse,
        tx: oneshot::Sender<()>,
    },
    Restarting(oneshot::Sender<()>),
    Restarted(oneshot::Sender<()>),
    SignalRestarted {
        join_response: proto::JoinResponse,
        tx: oneshot::Sender<()>,
    },
    Disconnected {
        reason: DisconnectReason,
    },
    LocalTrackSubscribed {
        track_sid: String,
    },
    DataStreamHeader {
        header: proto::data_stream::Header,
        participant_identity: String,
        encryption_type: proto::encryption::Type,
    },
    DataStreamChunk {
        chunk: proto::data_stream::Chunk,
        participant_identity: String,
        encryption_type: proto::encryption::Type,
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
    TrackMuted {
        sid: String,
        muted: bool,
    },
    LocalDataTrackInput(dt::local::InputEvent),
    RemoteDataTrackInput(dt::remote::InputEvent),
}

/// Represents a running RtcSession with the ability to close the session
/// and the engine_task
#[derive(Debug)]
struct EngineHandle {
    session: Arc<RtcSession>,
    closed: bool,
    reconnecting: bool,
    can_reconnect: bool,

    // If full_reconnect is true, the next attempt will not try to resume
    // and will instead do a full reconnect
    full_reconnect: bool,

    // The disconnect reason that started the current reconnection episode.
    // Carried through so that, if reconnection ultimately fails, the engine
    // closes with the original cause rather than a generic `UnknownReason`.
    reconnect_reason: DisconnectReason,
    engine_task: Option<(JoinHandle<()>, oneshot::Sender<()>)>,
}

struct EngineInner {
    // Keep a strong reference to LkRuntime to avoid creating a new RtcRuntime or PeerConnection
    // factory accross multiple Rtc sessions
    #[allow(dead_code)]
    lk_runtime: Arc<LkRuntime>,
    engine_tx: EngineEmitter,
    options: EngineOptions,

    close_notifier: Arc<Notify>,
    running_handle: RwLock<EngineHandle>,

    // The lock is write guarded for the whole reconnection time.
    // We can simply wait for reconnection by trying to acquire a read lock.
    // (This also prevents new reconnection to happens if a read guard is still held)
    reconnecting_lock: AsyncRwLock<()>,

    // Signalled when a server-requested reconnect wants the next attempt to fire
    // immediately, collapsing the exponential backoff wait between attempts.
    retry_now_notify: Arc<Notify>,
}

pub struct RtcEngine {
    inner: Arc<EngineInner>,
}

impl Debug for RtcEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtcEngine").finish()
    }
}

impl RtcEngine {
    pub async fn connect(
        url: &str,
        token: &str,
        options: EngineOptions,
        e2ee_manager: Option<E2eeManager>,
    ) -> EngineResult<(Self, proto::JoinResponse, EngineEvents)> {
        let (inner, join_response, engine_events) =
            EngineInner::connect(url, token, options, e2ee_manager).await?;
        Ok((Self { inner }, join_response, engine_events))
    }

    pub async fn close(&self, reason: DisconnectReason) {
        self.inner.close(reason).await
    }

    pub async fn publish_data(
        &self,
        data: proto::DataPacket,
        kind: DataPacketKind,
        is_raw_packet: bool,
    ) -> EngineResult<()> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.publish_data(data, kind, is_raw_packet).await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.simulate_scenario(scenario).await
    }

    pub async fn handle_local_data_track_output(
        &self,
        event: dt::local::OutputEvent,
    ) -> EngineResult<()> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.handle_local_data_track_output(event).await;
        Ok(())
    }

    pub async fn handle_remote_data_track_output(
        &self,
        event: dt::remote::OutputEvent,
    ) -> EngineResult<()> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.handle_remote_data_track_output(event).await;
        Ok(())
    }

    pub async fn add_track(&self, req: proto::AddTrackRequest) -> EngineResult<proto::TrackInfo> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.add_track(req).await
    }

    pub fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        // We don't need to wait for the reconnection
        let session = self.inner.running_handle.read().session.clone();
        session.remove_track(sender) // TODO(theomonnom): Ignore errors where this
                                     // RtpSender is bound to the old session. (Can
                                     // happen on bad timing and it is safe to ignore)
    }

    pub async fn mute_track(&self, req: proto::MuteTrackRequest) -> EngineResult<()> {
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };
        session.mute_track(req).await
    }

    pub async fn create_sender(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
        encodings: Vec<RtpEncodingParameters>,
    ) -> EngineResult<RtpTransceiver> {
        // When creating a new RtpSender, make sure we're always using the latest session
        let (session, _r_lock) = {
            let (handle, _r_lock) = self.inner.wait_reconnection().await?;
            (handle.session.clone(), _r_lock)
        };

        session.create_sender(track, options, encodings).await
    }

    pub fn publisher_negotiation_needed(&self) {
        let inner = self.inner.clone();
        livekit_runtime::spawn(async move {
            if let Ok((handle, _)) = inner.wait_reconnection().await {
                handle.session.publisher_negotiation_needed()
            }
        });
    }

    pub async fn send_request(&self, msg: proto::signal_request::Message) {
        // Getting the current session is OK to do without waiting for reconnection
        // SignalClient will attempt to queue the message if the session is not connected
        // Also on full_reconnect, every message is OK to ignore (Since this is another RtcSession)
        let session = self.inner.running_handle.read().session.clone();
        session.signal_client().send(msg).await // Returns () and automatically queues the message
                                                // on fail
    }

    pub async fn get_response(&self, request_id: u32) -> proto::RequestResponse {
        let session = self.inner.running_handle.read().session.clone();
        session.get_response(request_id).await
    }

    pub async fn get_stats(&self) -> EngineResult<SessionStats> {
        let session = self.inner.running_handle.read().session.clone();
        session.get_stats().await
    }

    pub fn session(&self) -> Arc<RtcSession> {
        self.inner.running_handle.read().session.clone()
    }
}

impl EngineInner {
    async fn connect(
        url: &str,
        token: &str,
        options: EngineOptions,
        e2ee_manager: Option<E2eeManager>,
    ) -> EngineResult<(Arc<Self>, proto::JoinResponse, EngineEvents)> {
        let lk_runtime = LkRuntime::instance();
        let max_retries = options.join_retries;

        let try_connect = {
            move || {
                let options = options.clone();
                let lk_runtime = lk_runtime.clone();
                let e2ee_manager = e2ee_manager.clone();
                async move {
                    let (session, join_response, session_events) =
                        RtcSession::connect(url, token, options.clone(), e2ee_manager).await?;
                    session.wait_pc_connection().await?;

                    let (engine_tx, engine_rx) = mpsc::unbounded_channel();
                    let inner = Arc::new(Self {
                        lk_runtime,
                        engine_tx,
                        close_notifier: Arc::new(Notify::new()),
                        running_handle: RwLock::new(EngineHandle {
                            session: Arc::new(session),
                            closed: false,
                            reconnecting: false,
                            can_reconnect: true,
                            full_reconnect: false,
                            reconnect_reason: DisconnectReason::UnknownReason,
                            engine_task: None,
                        }),
                        options,
                        reconnecting_lock: AsyncRwLock::default(),
                        retry_now_notify: Arc::new(Notify::new()),
                    });

                    // Start initial tasks
                    let (close_tx, close_rx) = oneshot::channel();
                    let session_task = livekit_runtime::spawn(Self::engine_task(
                        inner.clone(),
                        session_events,
                        close_rx,
                    ));
                    inner.running_handle.write().engine_task = Some((session_task, close_tx));

                    Ok((inner, join_response, engine_rx))
                }
            }
        };

        let mut last_error = None;
        for i in 0..(max_retries + 1) {
            match try_connect().await {
                Ok(res) => return Ok(res),
                Err(e) => {
                    let attempt_i = i + 1;
                    if i < max_retries {
                        log::warn!(
                            "failed to connect: {:?}, retrying... ({}/{})",
                            e,
                            attempt_i,
                            max_retries
                        );
                    }
                    last_error = Some(e)
                }
            }
        }

        Err(last_error.unwrap())
    }

    async fn engine_task(
        self: Arc<Self>,
        mut session_events: SessionEvents,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(event) = session_events.recv() => {
                    let debug = format!("{:?}", event);
                    let inner = self.clone();
                    let (tx, rx) = oneshot::channel();
                    let task = livekit_runtime::spawn(async move {
                        if let Err(err) = inner.on_session_event(event).await {
                            log::error!("failed to handle session event: {:?}", err);
                        }
                        let _ = tx.send(());
                    });

                    // Monitor sync/async blockings
                    tokio::select! {
                        _ = rx => {},
                        _ = livekit_runtime::sleep(Duration::from_secs(10)) => {
                            log::error!("session_event is taking too much time: {}", debug);
                        }
                    }

                    task.await;
                },
                 _ = &mut close_rx => {
                    break;
                }
            }
        }

        log::debug!("engine task closed");
    }

    async fn on_session_event(self: &Arc<Self>, event: SessionEvent) -> EngineResult<()> {
        match event {
            SessionEvent::Close { source, reason, action, retry_now } => {
                match action {
                    proto::leave_request::Action::Resume
                    | proto::leave_request::Action::Reconnect => {
                        {
                            let running_handle = self.running_handle.read();

                            // server could have sent a leave & disconnected signal client
                            // we don't want to start another resume cycle
                            if !running_handle.can_reconnect {
                                return Ok(());
                            }
                            // ensure we release the lock from this scope, it'll be used again in reconnection_needed
                        }

                        log::warn!(
                            "received session close: {:?} {:?} {:?}",
                            source,
                            reason,
                            action
                        );
                        self.reconnection_needed(
                            retry_now,
                            action == proto::leave_request::Action::Reconnect,
                            reason,
                        );
                    }
                    proto::leave_request::Action::Disconnect => {
                        // Disallow reconnection to avoid races
                        let mut running_handle = self.running_handle.write();
                        running_handle.can_reconnect = false;

                        // Spawning a new task because the close function wait for the engine_task to
                        // finish. (So it doesn't make sense to await it here)
                        livekit_runtime::spawn({
                            let inner = self.clone();
                            async move {
                                inner.close(reason).await;
                            }
                        });
                    }
                }
            }
            SessionEvent::Data {
                participant_sid,
                participant_identity,
                payload,
                topic,
                kind,
                encryption_type,
            } => {
                let _ = self.engine_tx.send(EngineEvent::Data {
                    participant_sid,
                    participant_identity,
                    payload,
                    topic,
                    kind,
                    encryption_type,
                });
            }
            SessionEvent::ChatMessage { participant_identity, message } => {
                let _ =
                    self.engine_tx.send(EngineEvent::ChatMessage { participant_identity, message });
            }
            SessionEvent::SipDTMF { participant_identity, code, digit } => {
                let _ =
                    self.engine_tx.send(EngineEvent::SipDTMF { participant_identity, code, digit });
            }
            SessionEvent::Transcription { participant_identity, track_sid, segments } => {
                let _ = self.engine_tx.send(EngineEvent::Transcription {
                    participant_identity,
                    track_sid,
                    segments,
                });
            }
            SessionEvent::SipDTMF { participant_identity, code, digit } => {
                let _ =
                    self.engine_tx.send(EngineEvent::SipDTMF { participant_identity, code, digit });
            }
            SessionEvent::RpcRequest {
                caller_identity,
                request_id,
                method,
                payload,
                response_timeout,
                version,
            } => {
                let _ = self.engine_tx.send(EngineEvent::RpcRequest {
                    caller_identity,
                    request_id,
                    method,
                    payload,
                    response_timeout,
                    version,
                });
            }
            SessionEvent::RpcResponse { request_id, payload, error } => {
                let _ =
                    self.engine_tx.send(EngineEvent::RpcResponse { request_id, payload, error });
            }
            SessionEvent::RpcAck { request_id } => {
                let _ = self.engine_tx.send(EngineEvent::RpcAck { request_id });
            }
            SessionEvent::MediaTrack { track, stream, transceiver } => {
                let _ = self.engine_tx.send(EngineEvent::MediaTrack { track, stream, transceiver });
            }
            SessionEvent::ParticipantUpdate { updates } => {
                let _ = self.engine_tx.send(EngineEvent::ParticipantUpdate { updates });
            }
            SessionEvent::SpeakersChanged { speakers } => {
                let _ = self.engine_tx.send(EngineEvent::SpeakersChanged { speakers });
            }
            SessionEvent::ConnectionQuality { updates } => {
                let _ = self.engine_tx.send(EngineEvent::ConnectionQuality { updates });
            }
            SessionEvent::RoomUpdate { room } => {
                let _ = self.engine_tx.send(EngineEvent::RoomUpdate { room });
            }
            SessionEvent::RoomMoved { moved } => {
                let _ = self.engine_tx.send(EngineEvent::RoomMoved { moved });
            }
            SessionEvent::LocalTrackSubscribed { track_sid } => {
                let _ = self.engine_tx.send(EngineEvent::LocalTrackSubscribed { track_sid });
            }
            SessionEvent::DataStreamHeader { header, participant_identity, encryption_type } => {
                let _ = self.engine_tx.send(EngineEvent::DataStreamHeader {
                    header,
                    participant_identity,
                    encryption_type,
                });
            }
            SessionEvent::DataStreamChunk { chunk, participant_identity, encryption_type } => {
                let _ = self.engine_tx.send(EngineEvent::DataStreamChunk {
                    chunk,
                    participant_identity,
                    encryption_type,
                });
            }
            SessionEvent::DataStreamTrailer { trailer, participant_identity } => {
                let _ = self
                    .engine_tx
                    .send(EngineEvent::DataStreamTrailer { trailer, participant_identity });
            }
            SessionEvent::DataChannelBufferedAmountLowThresholdChanged { kind, threshold } => {
                let _ = self.engine_tx.send(
                    EngineEvent::DataChannelBufferedAmountLowThresholdChanged { kind, threshold },
                );
            }
            SessionEvent::RefreshToken { url, token } => {
                let _ = self.engine_tx.send(EngineEvent::RefreshToken { url, token });
            }
            SessionEvent::TrackMuted { sid, muted } => {
                let _ = self.engine_tx.send(EngineEvent::TrackMuted { sid, muted });
            }
            SessionEvent::LocalDataTrackInput(event) => {
                let _ = self.engine_tx.send(EngineEvent::LocalDataTrackInput(event));
            }
            SessionEvent::RemoteDataTrackInput(event) => {
                let _ = self.engine_tx.send(EngineEvent::RemoteDataTrackInput(event));
            }
        }
        Ok(())
    }

    /// Close the engine
    /// the RtcSession is not removed so we can still access stats for e.g
    async fn close(&self, reason: DisconnectReason) {
        let (session, engine_task) = {
            let mut running_handle = self.running_handle.write();
            running_handle.closed = true;

            let session = running_handle.session.clone();
            let engine_task = running_handle.engine_task.take();
            (session, engine_task)
        };

        if let Some((engine_task, close_tx)) = engine_task {
            session.close(reason).await;
            let _ = close_tx.send(());
            let _ = engine_task.await;
        }

        // Always emit Disconnected, even when the engine_task was already taken by a
        // prior failed `try_restart_connection`. Without this, a reconnect cycle that
        // exhausts all attempts leaves the room stuck in Reconnecting forever because
        // the room's task never sees the event that drives `handle_disconnected`.
        let _ = self.engine_tx.send(EngineEvent::Disconnected { reason });

        // Signal any in-flight reconnect loop to stop. The reconnect task selects
        // on `close_notifier`, both at the top-level (cancelling the whole task)
        // and within its backoff wait (breaking the loop early). We notify LAST,
        // after teardown has completed: the reconnect loop's own bail paths call
        // `close()` from inside the task, so notifying earlier could let the
        // top-level select drop the task mid-`close()` and leave teardown partial.
        self.close_notifier.notify_waiters();
    }

    /// When waiting for reconnection, it ensures we're always using the latest session.
    async fn wait_reconnection(
        &self,
    ) -> EngineResult<(RwLockReadGuard<EngineHandle>, AsyncRwLockReadGuard<()>)> {
        let r_lock = self.reconnecting_lock.read().await;
        let running_handle = self.running_handle.read();

        if running_handle.closed {
            // Reconnection may have failed
            // TODO(theomonnom): More precise error?
            return Err(EngineError::Connection("engine is closed".into()));
        }

        Ok((running_handle, r_lock))
    }

    /// Start the reconnect task if not already started
    /// Ask to retry directly if `retry_now` is true
    /// Ask for a full reconnect if `full_reconnect` is true
    /// `reason` is the disconnect cause that triggered this reconnection
    fn reconnection_needed(
        self: &Arc<Self>,
        retry_now: bool,
        full_reconnect: bool,
        reason: DisconnectReason,
    ) {
        let mut running_handle = self.running_handle.write();

        if !running_handle.can_reconnect {
            return;
        }

        if running_handle.reconnecting {
            // Only escalate to full reconnect, never downgrade. Stale signal-close
            // events (which request resume) must not override a full reconnect decision
            // made by the reconnect loop after a failed resume attempt.
            if full_reconnect {
                running_handle.full_reconnect = true;
            }

            // Wake the in-flight reconnect loop so its next attempt fires
            // immediately, collapsing the backoff wait.
            if retry_now {
                self.retry_now_notify.notify_one();
            }

            return;
        }

        running_handle.reconnecting = true;
        running_handle.full_reconnect = full_reconnect;
        // Remember the cause so a failed reconnection closes with it rather than
        // a generic UnknownReason.
        running_handle.reconnect_reason = reason;

        livekit_runtime::spawn({
            let inner = self.clone();
            async move {
                // Hold the reconnection lock for the whole reconnection time
                let _r_lock = inner.reconnecting_lock.write().await;
                // The close function can send a signal to cancel the reconnection

                let close_notifier = inner.close_notifier.clone();
                let close_receiver = close_notifier.notified();
                tokio::pin!(close_receiver);

                tokio::select! {
                    _ = &mut close_receiver => {
                        // The engine was closed; abandon the reconnect attempt.
                        // Clear `reconnecting` (the success/failure path below does
                        // this after the select; this branch returns early so it
                        // must do so itself) to avoid leaving a closed engine stuck
                        // with reconnecting = true.
                        log::debug!("reconnection cancelled");
                        inner.running_handle.write().reconnecting = false;
                        return;
                    }
                    res = inner.reconnect_task() => {
                        if res.is_err() {
                            log::error!("failed to reconnect to the livekit room");
                            // The loop may already have closed the engine with an
                            // accurate reason (e.g. a server Disconnect hit
                            // mid-attempt). Only close here for the paths that
                            // didn't — chiefly attempt exhaustion — and do so with
                            // the cause that started this episode rather than a
                            // generic UnknownReason, avoiding a duplicate
                            // Disconnected event with a stale reason.
                            let (already_closed, reason) = {
                                let handle = inner.running_handle.read();
                                (handle.closed, handle.reconnect_reason)
                            };
                            if !already_closed {
                                inner.close(reason).await;
                            }
                        } else {
                            log::info!("RtcEngine successfully recovered")
                        }
                    }
                }

                let mut running_handle = inner.running_handle.write();
                running_handle.reconnecting = false;

                // r_lock is now dropped
            }
        });
    }

    /// Runned every time the PeerConnection or the SignalClient is closed
    /// We first try to resume the connection, if it fails, we start a full reconnect.
    /// NOTE: The reconnect_task must be canncellation safe
    async fn reconnect_task(self: &Arc<Self>) -> EngineResult<()> {
        // Get the latest connection info from the signal_client (including the refreshed token
        // because the initial join token may have expired)
        let (url, token, e2ee_manager) = {
            let running_handle = self.running_handle.read();
            let signal_client = running_handle.session.signal_client();
            let e2ee_manager = running_handle.session.e2ee_manager();
            (
                signal_client.url(),
                signal_client.token(), // Refreshed token
                e2ee_manager.clone(),
            )
        };

        // Lifecycle notifications are emitted once per mode: Resuming the first
        // time the episode resumes, Restarting the first time it (re)enters full
        // reconnect. Crucially this includes an escalation from a failed resume,
        // which previously emitted no Restarting at all -- leaving the Room to
        // observe Resuming followed by Restarted with no Restarting between
        // (DELTA 2).
        let mut resuming_emitted = false;
        let mut restarting_emitted = false;

        for i in 1..=RECONNECT_ATTEMPTS {
            let (is_closed, full_reconnect) = {
                let running_handle = self.running_handle.read();
                (running_handle.closed, running_handle.full_reconnect)
            };

            if is_closed {
                return Err(EngineError::Connection("attempt canncelled, engine is closed".into()));
            }

            if full_reconnect {
                if !restarting_emitted {
                    restarting_emitted = true;
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_tx.send(EngineEvent::Restarting(tx));
                    let _ = rx.await;
                }

                log::error!("restarting connection... attempt: {}", i);
                match self
                    .try_restart_connection(
                        &url,
                        &token,
                        self.options.clone(),
                        e2ee_manager.clone(),
                    )
                    .await
                {
                    Ok(()) => {
                        let (tx, rx) = oneshot::channel();
                        let _ = self.engine_tx.send(EngineEvent::Restarted(tx));
                        let _ = rx.await;
                        return Ok(());
                    }
                    Err(err) => {
                        if let Some(reason) = leave_disconnect_reason(&err) {
                            log::warn!("server requested disconnect during restart: {:?}", reason);
                            self.running_handle.write().can_reconnect = false;
                            self.close(reason).await;
                            return Err(EngineError::Connection(
                                "server requested disconnect during restart".into(),
                            ));
                        }
                        log::error!("restarting connection failed: {}", err);
                    }
                }
            } else {
                if !resuming_emitted {
                    resuming_emitted = true;
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_tx.send(EngineEvent::Resuming(tx));
                    let _ = rx.await;
                }

                log::error!("resuming connection... attempt: {}", i);
                match self.try_resume_connection().await {
                    Ok(()) => {
                        let (tx, rx) = oneshot::channel();
                        let _ = self.engine_tx.send(EngineEvent::Resumed(tx));
                        let _ = rx.await;
                        return Ok(());
                    }
                    Err(err) => {
                        if let Some(reason) = leave_disconnect_reason(&err) {
                            log::warn!("server requested disconnect during resume: {:?}", reason);
                            self.running_handle.write().can_reconnect = false;
                            self.close(reason).await;
                            return Err(EngineError::Connection(
                                "server requested disconnect during resume".into(),
                            ));
                        }
                        log::error!("resuming connection failed: {}", err);
                        let mut running_handle = self.running_handle.write();
                        running_handle.full_reconnect = true;
                    }
                }
            }

            // Exponential backoff with full jitter between attempts (DELTA 3).
            // A server-requested reconnect signals retry_now_notify to collapse
            // this wait so the next attempt fires immediately; a close signals
            // close_notifier to break out of the loop early (the next iteration's
            // `is_closed` check then returns) instead of waiting out the backoff.
            let backoff = reconnect_strategy::delay(i);
            tokio::select! {
                _ = livekit_runtime::sleep(backoff) => {}
                _ = self.retry_now_notify.notified() => {
                    log::debug!("retry_now signalled, skipping reconnect backoff");
                }
                _ = self.close_notifier.notified() => {
                    log::debug!("engine closed, cancelling reconnect backoff");
                }
            }
        }

        Err(EngineError::Connection(
            format!("failed to reconnect after {}", RECONNECT_ATTEMPTS).into(),
        ))
    }

    /// Try to recover the connection by doing a full reconnect.
    /// It recreates a new RtcSession (new peer connection, new signal client, new data channels,
    /// etc...)
    async fn try_restart_connection(
        self: &Arc<Self>,
        url: &str,
        token: &str,
        options: EngineOptions,
        e2ee_manager: Option<E2eeManager>,
    ) -> EngineResult<()> {
        // Close the current RtcSession and the current tasks
        let (session, engine_task) = {
            let mut running_handle = self.running_handle.write();
            let session = running_handle.session.clone();
            let engine_task = running_handle.engine_task.take();
            (session, engine_task)
        };

        if let Some((engine_task, close_tx)) = engine_task {
            session.close(DisconnectReason::ClientInitiated).await;
            let _ = close_tx.send(());
            let _ = engine_task.await;
        }

        let (new_session, join_response, session_events) =
            RtcSession::connect(url, token, options, e2ee_manager).await?;

        // On SignalRestarted, the room will try to unpublish the local tracks
        // NOTE: Doing operations that use rtc_session will not use the new one
        let (tx, rx) = oneshot::channel();
        let _ = self.engine_tx.send(EngineEvent::SignalRestarted { join_response, tx });
        let _ = rx.await;

        new_session.wait_pc_connection().await?;

        // Only replace the current session if the new one succeed
        // This is important so we can still use the old session if the new one failed
        // (for example, this is important if we still want to get the stats of the old session)
        // This has the drawback to not being able to use the new session on the SignalRestarted
        // event.
        let mut handle = self.running_handle.write();
        handle.session = Arc::new(new_session);

        let (close_tx, close_rx) = oneshot::channel();
        let task = livekit_runtime::spawn(self.clone().engine_task(session_events, close_rx));
        handle.engine_task = Some((task, close_tx));

        Ok(())
    }

    /// Resume the current session in place (the lightweight reconnect path).
    ///
    /// The steps below run in a fixed order that any change must preserve, and
    /// each non-trivial seam is its own method so the sequence — and the reason
    /// for the ordering — is explicit rather than implied by statement order.
    /// Mirrors the resume chain in `livekit/specs/signalling-reconnection.allium`:
    ///   1. reopen the signalling link (queue gate stays on until step 4);
    ///   2. SyncState before the publisher re-offer;
    ///   3. re-offer the publisher, then await PC reconnection + settle;
    ///   4. re-check link liveness, then drain the queue.
    async fn try_resume_connection(&self) -> EngineResult<()> {
        let session = self.running_handle.read().session.clone();

        // 1. Reopen the signalling link. The SignalClient stays gated
        //    (`reconnecting=true`) so queueable mutations buffer until step 4.
        let reconnect_response = session.restart().await?;

        // 2. Hand the ReconnectResponse to the room and wait until it has sent
        //    SyncState, which must precede the publisher re-offer.
        self.resume_sync_state(reconnect_response).await;

        // 3. Re-offer the publisher (strictly AFTER SyncState) and wait for the
        //    PeerConnections to reconnect, applying the settle delay.
        session.restart_publisher().await?;
        session.wait_pc_reconnected(PC_RECONNECT_SETTLE_DELAY).await?;

        // 4. Re-check link liveness and drain the queued mutations.
        self.resume_finalize(&session).await
    }

    /// Resume step 2: announce the resume to the room and block until it has
    /// sent SyncState. SyncState is a pass-through signal, so it reaches the
    /// server immediately even though the SignalClient is still gated.
    async fn resume_sync_state(&self, reconnect_response: proto::ReconnectResponse) {
        let (tx, rx) = oneshot::channel();
        let _ = self.engine_tx.send(EngineEvent::SignalResumed { reconnect_response, tx });
        // The room replies on `tx` once SyncState has gone out.
        let _ = rx.await;
    }

    /// Resume step 4: confirm the signalling link survived the PC-reconnect wait
    /// before draining the queue. If the WS died while we were waiting for the
    /// PeerConnections, draining queued mutations would just push them into the
    /// void; bail instead and let the engine try a fresh resume (or escalate).
    async fn resume_finalize(&self, session: &RtcSession) -> EngineResult<()> {
        if !session.signal_client().is_connected().await {
            return Err(EngineError::Connection("signal connection severed during resume".into()));
        }

        // Flush queued mutations and clear the `reconnecting` gate — the resume
        // has fully recovered, so deferred subscription updates / mutes / etc.
        // should now reach the server. Mirrors `client.setReconnected()`.
        session.signal_client().set_reconnected().await;
        Ok(())
    }
}

impl From<livekit_datatrack::api::InternalError> for EngineError {
    fn from(err: livekit_datatrack::api::InternalError) -> Self {
        Self::Internal(err.to_string().into())
    }
}

/// Inspect a reconnect-attempt error and return the server-supplied disconnect
/// reason iff the server sent `LeaveRequest{action: Disconnect}` while we were
/// trying to (re)connect. In that case the reconnect loop should bail out
/// rather than escalate to a full reconnect — the server is explicitly telling
/// us to stop trying. `Reconnect`/`Resume` actions still fall through to the
/// normal escalation path.
fn leave_disconnect_reason(err: &EngineError) -> Option<DisconnectReason> {
    if let EngineError::Signal(SignalError::LeaveRequest { reason, action }) = err {
        if *action == proto::leave_request::Action::Disconnect {
            return Some(*reason);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leave_disconnect_reason_returns_some_only_for_disconnect_action() {
        let disconnect_err = EngineError::Signal(SignalError::LeaveRequest {
            reason: DisconnectReason::ServerShutdown,
            action: proto::leave_request::Action::Disconnect,
        });
        assert_eq!(
            leave_disconnect_reason(&disconnect_err),
            Some(DisconnectReason::ServerShutdown),
            "Disconnect action should propagate the server reason"
        );

        for action in
            [proto::leave_request::Action::Reconnect, proto::leave_request::Action::Resume]
        {
            let err = EngineError::Signal(SignalError::LeaveRequest {
                reason: DisconnectReason::ServerShutdown,
                action,
            });
            assert!(
                leave_disconnect_reason(&err).is_none(),
                "{:?} action must NOT short-circuit the reconnect loop",
                action
            );
        }
    }

    #[test]
    fn leave_disconnect_reason_ignores_non_leave_errors() {
        let other_errors = [
            EngineError::Connection("network".into()),
            EngineError::Internal("bug".into()),
            EngineError::Signal(SignalError::SendError),
            EngineError::Signal(SignalError::Timeout("waiting".into())),
        ];
        for err in &other_errors {
            assert!(
                leave_disconnect_reason(err).is_none(),
                "{:?} must not be treated as a disconnect Leave",
                err
            );
        }
    }
}
