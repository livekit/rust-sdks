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

use crate::id::ParticipantSid;
use crate::options::TrackPublishOptions;
use crate::prelude::LocalTrack;
use crate::room::DisconnectReason;
use crate::rtc_engine::lk_runtime::LkRuntime;
use crate::rtc_engine::rtc_session::{RtcSession, SessionEvent, SessionEvents};
use crate::DataPacketKind;
use futures_util::lock::MutexGuard;
use libwebrtc::prelude::*;
use libwebrtc::session_description::SdpParseError;
use livekit_api::signal_client::{SignalError, SignalOptions};
use livekit_protocol as proto;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tokio::sync::{RwLock as AsyncRwLock, RwLockReadGuard};
use tokio::task::JoinHandle;
use tokio::time::{interval, Interval, MissedTickBehavior};

pub mod lk_runtime;
mod peer_transport;
mod rtc_events;
mod rtc_session;

pub(crate) type EngineEmitter = mpsc::UnboundedSender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::UnboundedReceiver<EngineEvent>;
pub(crate) type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SimulateScenario {
    SignalReconnect,
    Speaker,
    NodeFailure,
    ServerLeave,
    Migration,
    ForceTcp,
    ForceTls,
}

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("signal failure: {0}")]
    Signal(#[from] SignalError),
    #[error("internal webrtc failure")]
    Rtc(#[from] RtcError),
    #[error("failed to parse sdp")]
    Parse(#[from] SdpParseError),
    #[error("serde error")]
    Serde(#[from] serde_json::Error),
    #[error("failed to send data to the datachannel")]
    Data(#[from] DataChannelError),
    #[error("connection error: {0}")]
    Connection(String),
    #[error("decode error")]
    Decode(#[from] prost::DecodeError),
    #[error("internal error: {0}")]
    Internal(String), // Unexpected error
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
        participant_sid: ParticipantSid,
        payload: Vec<u8>,
        kind: DataPacketKind,
    },
    SpeakersChanged {
        speakers: Vec<proto::SpeakerInfo>,
    },
    ConnectionQuality {
        updates: Vec<proto::ConnectionQualityInfo>,
    },

    /// The following events are used to notify the room about the reconnection state
    /// Since the room needs to also sync state in a good timing with the server.
    /// We synchronize the state with a one-shot channel.
    Resuming(oneshot::Sender<()>),
    Resumed(oneshot::Sender<()>),
    SignalResumed(oneshot::Sender<()>),
    Restarting(oneshot::Sender<()>),
    Restarted(oneshot::Sender<()>),
    SignalRestarted(oneshot::Sender<()>),

    Disconnected {
        reason: DisconnectReason,
    },
}

#[derive(Clone, Debug)]
enum ReconnectionEvent {
    Resuming,   // Starting a new resume attempt
    Restarting, // Starting a new full reconnect attempt
    Reconnected,
    Closed,
}

pub const RECONNECT_ATTEMPTS: u32 = 10;
pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

/// Represents a running RtcSession with the ability to close the session
/// and the engine_task
#[derive(Debug)]
struct EngineHandle {
    session: RtcSession,
    closed: bool,
    reconnecting: bool,

    // If full_reconnect is true, the next attempt will not try to resume
    // and will instead do a full reconnect
    full_reconnect: bool,
    engine_task: Option<(JoinHandle<()>, oneshot::Sender<()>)>,
}

#[derive(Default, Debug, Clone)]
pub struct EngineOptions {
    pub rtc_config: RtcConfiguration,
    pub signal_options: SignalOptions,
}

#[derive(Default, Debug, Clone)]
pub struct LastInfo {
    // The join response is updated each time a full reconnect is done
    pub join_response: proto::JoinResponse,

    // The last offer/answer exchanged during the last session
    pub subscriber_offer: Option<SessionDescription>,
    pub subscriber_answer: Option<SessionDescription>,

    pub data_channels_info: Vec<proto::DataChannelInfo>,
}

struct EngineInner {
    // Keep a strong reference to LkRuntime to avoid creating a new RtcRuntime or PeerConnection factory accross multiple Rtc sessions
    _lk_runtime: Arc<LkRuntime>,
    engine_tx: EngineEmitter,
    options: EngineOptions,

    // Last/current session states (needed by the room)
    // last_info: Mutex<LastInfo>,

    // Write lock to the running_handle should be fast
    // since it's only used when the engine is closed
    running_handle: AsyncRwLock<EngineHandle>,

    waiting_reconnection: AtomicU32,
    reconnect_interval: AsyncMutex<Interval>,
    reconnect_watcher: watch::Sender<ReconnectionEvent>,
}

impl Debug for EngineInner {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("EngineInner")
            .field("closed", &self.closed)
            .field("reconnecting", &self.reconnecting)
            .field("full_reconnect", &self.full_reconnect)
            .finish()
    }
}

#[derive(Debug)]
pub struct RtcEngine {
    inner: Arc<EngineInner>,
}

impl RtcEngine {
    pub async fn connect(
        url: &str,
        token: &str,
        options: EngineOptions,
    ) -> EngineResult<(Self, EngineEvents)> {
        let (engine_emitter, engine_events) = mpsc::channel(8);

        let mut reconnect_interval = interval(RECONNECT_INTERVAL);
        reconnect_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Connect to the initial session
        let (session, session_events) = RtcSession::connect(url, token, options).await?;

        let inner = Arc::new(EngineInner {
            _lk_runtime: LkRuntime::instance(),
            running_handle: Default::default(),

            engine_emitter,

            engine_handle: RwLock::new(EngineHandle {
                session,
                engine_task: None,
            }),

            options: options.clone(),

            last_info: Default::default(),
            closed: Default::default(),
            reconnecting: Default::default(),
            full_reconnect: Default::default(),

            reconnect_interval: AsyncMutex::new(reconnect_interval),
            reconnect_notifier: Arc::new(Notify::new()),
        });

        // Start initial tasks
        let (close_tx, close_rx) = oneshot::channel();
        let session_task = tokio::spawn(session_task(inner.clone(), session_events, close_rx));

        Ok((Self { inner }, engine_events))
    }

    pub async fn close(&self) {
        self.inner.close(DisconnectReason::ClientInitiated).await
    }

    pub async fn publish_data(
        &self,
        data: &proto::DataPacket,
        kind: DataPacketKind,
    ) -> EngineResult<()> {
        // Make sure we are connected before trying to send data
        self.inner.wait_reconnection().await?;
        let handle = self.inner.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.publish_data(data, kind).await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.wait_reconnection().await?;
        let handle = self.inner.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.simulate_scenario(scenario).await
    }

    pub async fn add_track(&self, req: proto::AddTrackRequest) -> EngineResult<proto::TrackInfo> {
        self.inner.wait_reconnection().await?;
        let handle = self.inner.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.add_track(req).await
    }

    pub async fn remove_track(&self, sender: RtpSender) -> EngineResult<()> {
        self.inner.wait_reconnection().await?;
        let handle = self.inner.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.remove_track(sender).await
    }

    pub async fn create_sender(
        &self,
        track: LocalTrack,
        options: TrackPublishOptions,
        encodings: Vec<RtpEncodingParameters>,
    ) -> EngineResult<RtpTransceiver> {
        self.inner.wait_reconnection().await?;
        let handle = self.inner.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.create_sender(track, options, encodings).await
    }

    pub fn publisher_negotiation_needed(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            if inner.wait_reconnection().await.is_ok() {
                let handle = inner.running_handle.read().await;
                let session = &handle.as_ref().unwrap().session;
                session.publisher_negotiation_needed()
            }
        });
    }

    pub async fn send_request(&self, msg: proto::signal_request::Message) -> EngineResult<()> {
        let handle = self.inner.running_handle.read().await;

        if let Some(handle) = handle.as_ref() {
            handle.session.signal_client().send(msg).await;
        } else {
            // Should be OK to ignore (full reconnect)
        }
        Ok(())
    }

    pub fn last_info(&self) -> LastInfo {
        self.inner.last_info.lock().clone()
    }
}

impl EngineInner {
    async fn connect(
        self,
        url: &str,
        token: &str,
        options: EngineOptions,
    ) -> EngineResult<Arc<Self>> {
        let (session, session_events) = RtcSession::connect(url, token, options.clone()).await?;
        let (engine_tx, engine_rx) = mpsc::unbounded_channel();

        let mut inner = Arc::new(Self {
            _lk_runtime: LkRuntime::instance(),
            engine_tx,

            running_handle: AsyncRwLock::new(EngineHandle {
                session,
                engine_task: None,
            }),

            options,

            // TODO, remove this
            last_info: Default::default(),
            closed: Default::default(),
            reconnecting: Default::default(),
            full_reconnect: Default::default(),

            reconnect_interval: AsyncMutex::new(interval(RECONNECT_INTERVAL)),
            reconnect_notifier: Arc::new(Notify::new()),
        });

        // Start initial tasks
        let (close_tx, close_rx) = oneshot::channel();
        let session_task = tokio::spawn(Self::engine_task(inner.clone(), session_events, close_rx));

        inner.running_handle.write().await.engine_task = Some((session_task, close_tx));

        Ok(inner)
    }

    async fn wait_reconnection(&self) -> EngineResult<RwLockReadGuard<EngineHandle>> {
        let running_handle = self.running_handle.read().await;
        if !running_handle.reconnecting {
            return Ok(running_handle);
        }

        // What prevents a new reconnection?
        // 1. Holding a running_handle lock
        //    - This is the reason why we return a RwLockReadGuard<RtcSession> so the reconnection
        //    logic can't acquire the lock between the end of this function and the next
        //    caller/user code
        // 2. wait_reconnection to be equals to 0

        // The reason why we can't just keep the lock for the whole function, is that some operations
        // like removing a RtpSender doesn't necessary need to wait for the reconnection
        // and we want to use the current RtcSession/PeerConnections

        self.waiting_reconnection.fetch_add(1, Ordering::Release); // Prevent a new
                                                                   // reconnection to happen even if we don't hold the running_handle lock

        let rx = self.reconnect_watcher.subscribe();
        // Drop the lock so some operations can still be executed while we reconnect
        // Also we subscribe before dropping the lock so the reconnection logic can't send
        // a notification before we subscribe and miss it.
        drop(running_handle);

        // Wait for the reconnection to finish
        loop {
            let _ = rx.changed().await;
            let reconnection_state = rx.borrow()?;
        }

        // Reconnection finished, reacquire the lock
        let running_handle = self.running_handle.read().await;
        self.waiting_reconnection.fetch_sub(1, Ordering::Acquire);
        Ok(running_handle)
    }

    async fn engine_task(
        self: Arc<Self>,
        mut session_events: SessionEvents,
        mut close_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(event) = session_events.recv() => {
                    let debug = format!("{:?}", event);
                    let inner = self.clone();
                    let (tx, rx) = oneshot::channel();
                    let task = tokio::spawn(async move {
                        if let Err(err) = inner.on_session_event(event).await {
                            log::error!("failed to handle session event: {:?}", err);
                        }
                        let _ = tx.send(());
                    });

                    // Monitor sync/async blockings
                    tokio::select! {
                        _ = rx => {},
                        _ = tokio::time::sleep(Duration::from_secs(10)) => {
                            log::error!("session_event is taking too much time: {}", debug);
                        }
                    }

                    task.await.unwrap();
                },
                 _ = &mut close_receiver => {
                    break;
                }
            }
        }

        log::debug!("engine task closed");
    }

    async fn on_session_event(self: &Arc<Self>, event: SessionEvent) -> EngineResult<()> {
        match event {
            SessionEvent::Close {
                source,
                reason,
                can_reconnect,
                retry_now,
                full_reconnect,
            } => {
                log::info!("received session close: {}, {:?}", source, reason);
                if can_reconnect {
                    self.try_reconnect(retry_now, full_reconnect);
                } else {
                    // Spawning a new task because the close function wait for the engine_task to
                    // finish. (Where this function is called from)
                    tokio::spawn({
                        let inner = self.clone();
                        async move {
                            inner.close(reason).await;
                        }
                    });
                }
            }
            SessionEvent::Data {
                participant_sid,
                payload,
                kind,
            } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::Data {
                        participant_sid,
                        payload,
                        kind,
                    })
                    .await;
            }
            SessionEvent::MediaTrack {
                track,
                stream,
                transceiver,
            } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::MediaTrack {
                        track,
                        stream,
                        transceiver,
                    })
                    .await;
            }
            SessionEvent::ParticipantUpdate { updates } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::ParticipantUpdate { updates })
                    .await;
            }
            SessionEvent::SpeakersChanged { speakers } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::SpeakersChanged { speakers })
                    .await;
            }
            SessionEvent::ConnectionQuality { updates } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::ConnectionQuality { updates })
                    .await;
            }
        }
        Ok(())
    }

    /* async fn connect(
        self: &Arc<Self>,
        url: &str,
        token: &str,
        options: EngineOptions,
    ) -> EngineResult<()> {
        let mut running_handle = self.running_handle.write().await;
        if running_handle.is_some() {
            panic!("engine is already connected");
        }

        let (session, session_events) = RtcSession::connect(url, token, options).await?;

        let (close_sender, close_receiver) = oneshot::channel();
        let engine_task = tokio::spawn(self.clone().engine_task(session_events, close_receiver));

        let engine_handle = EngineHandle {
            session,
            engine_task,
            close_sender,
        };

        *running_handle = Some(engine_handle);

        // Always update the join response after a new session is created (first session or full reconnect)
        drop(running_handle);
        self.update_last_info().await;

        Ok(())
    }*/

    async fn update_last_info(&self) {
        if let Some(handle) = self.running_handle.read().await.as_ref() {
            let mut last_info = self.last_info.lock();
            let subscriber_pc = handle.session.subscriber().peer_connection();

            last_info.join_response = handle.session.signal_client().join_response();
            last_info.subscriber_offer = subscriber_pc.current_remote_description();
            last_info.subscriber_answer = subscriber_pc.current_local_description();
            last_info.data_channels_info = handle.session.data_channels_info();
        }
    }

    async fn terminate_session(&self) {
        if let Some(handle) = self.running_handle.write().await.take() {
            handle.session.close().await;
            let _ = handle.close_sender.send(());
            let _ = handle.engine_task.await;
        }
    }

    async fn close(&self, reason: DisconnectReason) {
        self.closed.store(true, Ordering::Release);
        self.terminate_session().await;
        let _ = self
            .engine_emitter
            .send(EngineEvent::Disconnected { reason })
            .await;
    }

    /// Start the reconnect task if not already started
    /// Ask to retry directly if `retry_now` is true
    /// Ask for a full reconnect if `full_reconnect` is true
    fn try_reconnect(self: &Arc<Self>, retry_now: bool, full_reconnect: bool) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        self.full_reconnect.store(full_reconnect, Ordering::Release);
        let inner = self.clone();
        if retry_now {
            tokio::spawn(async move {
                inner.reconnect_interval.lock().await.reset();
            });
        }

        if self
            .reconnecting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        tokio::spawn({
            let inner = self.clone();
            async move {
                // Reconnetion logic
                inner.reconnect_interval.lock().await.reset();
                inner
                    .full_reconnect
                    .store(full_reconnect, Ordering::Release);

                let res = inner.reconnect_task().await; // Wait for the reconnection task to finish
                inner.reconnecting.store(false, Ordering::Release);

                if res.is_ok() {
                    log::info!("RtcEngine successfully recovered")
                } else {
                    log::error!("failed to reconnect after {} attempts", RECONNECT_ATTEMPTS);
                    inner.close(DisconnectReason::UnknownReason).await;
                }

                inner.reconnect_notifier.notify_waiters();
            }
        });
    }

    /// Runned every time the PeerConnection or the SignalClient is closed
    /// We first try to resume the connection, if it fails, we start a full reconnect.
    async fn reconnect_task(self: &Arc<Self>) -> EngineResult<()> {
        // Get the latest connection info from the signal_client (including the refreshed token because the initial join token may have expired)
        let (url, token) = {
            let running_handle = self.running_handle.read().await;
            let signal_client = running_handle.as_ref().unwrap().session.signal_client();
            (
                signal_client.url(),
                signal_client.token(), // Refreshed token
            )
        };

        // Update last info before trying to reconnect/resume
        self.update_last_info().await;

        for i in 0..RECONNECT_ATTEMPTS {
            if self.closed.load(Ordering::Acquire) {
                // The user closed the RTCEngine, cancel the reconnection task
                return Ok(());
            }

            if self.full_reconnect.load(Ordering::SeqCst) {
                if i == 0 {
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_emitter.send(EngineEvent::Restarting(tx)).await;
                    let _ = rx.await;
                }

                log::error!("restarting connection... attempt: {}", i);
                if let Err(err) = self
                    .try_restart_connection(&url, &token, self.options.clone())
                    .await
                {
                    log::error!("restarting connection failed: {}", err);
                } else {
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_emitter.send(EngineEvent::Restarted(tx)).await;
                    let _ = rx.await;
                    return Ok(());
                }
            } else {
                if i == 0 {
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_emitter.send(EngineEvent::Resuming(tx)).await;
                    let _ = rx.await;
                }

                log::error!("resuming connection... attempt: {}", i);
                if let Err(err) = self.try_resume_connection().await {
                    log::error!("resuming connection failed: {}", err);
                    if let EngineError::Signal(_) = err {
                        self.full_reconnect.store(true, Ordering::SeqCst);
                    }
                } else {
                    let (tx, rx) = oneshot::channel();
                    let _ = self.engine_emitter.send(EngineEvent::Resumed(tx)).await;
                    let _ = rx.await;
                    return Ok(());
                }
            }

            self.reconnect_interval.lock().await.tick().await;
        }

        Err(EngineError::Connection("failed to reconnect".to_owned()))
    }

    /// Try to recover the connection by doing a full reconnect.
    /// It recreates a new RtcSession
    async fn try_restart_connection(
        self: &Arc<Self>,
        url: &str,
        token: &str,
        options: EngineOptions,
    ) -> EngineResult<()> {
        self.terminate_session().await; // Invalid because we want the current RtcSession to still
                                        // be available even if the full reconnect failed (next line)
        self.connect(url, token, options).await?;

        let (tx, rx) = oneshot::channel();
        let _ = self
            .engine_emitter
            .send(EngineEvent::SignalRestarted(tx))
            .await;
        let _ = rx.await;

        let handle = self.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;
        session.wait_pc_connection().await
    }

    /// Try to restart the current session
    async fn try_resume_connection(&self) -> EngineResult<()> {
        let handle = self.running_handle.read().await;
        let session = &handle.as_ref().unwrap().session;

        session.restart().await?;

        let (tx, rx) = oneshot::channel();
        let _ = self
            .engine_emitter
            .send(EngineEvent::SignalResumed(tx))
            .await;

        // With SignalResumed, the room will send a SyncState message to the server
        let _ = rx.await;

        // The publisher offer must be sent AFTER the SyncState message
        session.restart_publisher().await?;
        session.wait_pc_connection().await
    }
}
