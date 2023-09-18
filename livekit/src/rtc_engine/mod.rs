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
use livekit_api::signal_client::{SignalError, SignalOptions};
use livekit_protocol as proto;
use livekit_webrtc::prelude::*;
use livekit_webrtc::session_description::SdpParseError;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock as AsyncRwLock;
use tokio::sync::{mpsc, oneshot};
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::{interval, Interval, MissedTickBehavior};

pub mod lk_runtime;
mod peer_transport;
mod rtc_events;
mod rtc_session;

pub(crate) type EngineEmitter = mpsc::Sender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::Receiver<EngineEvent>;
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
        receiver: RtpReceiver,
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

pub const RECONNECT_ATTEMPTS: u32 = 10;
pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

/// Represents a running RTCSession with the ability to close the session
/// and the engine_task
#[derive(Debug)]
struct EngineHandle {
    session: RtcSession,
    engine_task: JoinHandle<()>,
    close_sender: oneshot::Sender<()>,
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
    #[allow(dead_code)]
    lk_runtime: Arc<LkRuntime>,
    engine_emitter: EngineEmitter,

    // Last/current session states (needed by the room)
    last_info: Mutex<LastInfo>,
    running_handle: AsyncRwLock<Option<EngineHandle>>,

    // Reconnecting fields
    closed: AtomicBool, // True if closed or the reconnection failed (Note that this is false when reconnecting or resuming)
    reconnecting: AtomicBool,
    full_reconnect: AtomicBool, // If true, the next reconnect attempt will skip resume and directly try a full reconnect
    reconnect_interval: AsyncMutex<Interval>,
    reconnect_notifier: Arc<Notify>, // Called when the reconnection task finisehd, successful or not
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
        options: SignalOptions,
    ) -> EngineResult<(Self, EngineEvents)> {
        let (engine_emitter, engine_events) = mpsc::channel(8);

        let mut reconnect_interval = interval(RECONNECT_INTERVAL);
        reconnect_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let inner = Arc::new(EngineInner {
            lk_runtime: LkRuntime::instance(),
            running_handle: Default::default(),
            engine_emitter,

            last_info: Default::default(),
            closed: Default::default(),
            reconnecting: Default::default(),
            full_reconnect: Default::default(),

            reconnect_interval: AsyncMutex::new(reconnect_interval),
            reconnect_notifier: Arc::new(Notify::new()),
        });

        inner.connect(url, token, options).await?;
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
    async fn engine_task(
        self: Arc<Self>,
        mut session_events: SessionEvents,
        mut close_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                res = session_events.recv() => {
                    if let Some(event) = res {
                        if let Err(err) = self.on_session_event(event).await {
                            log::error!("failed to handle session event: {:?}", err);
                        }
                    }
                },
                 _ = &mut close_receiver => {
                    log::trace!("closing engine task");
                    break;
                }
            }
        }
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
                receiver,
                transceiver,
            } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::MediaTrack {
                        track,
                        stream,
                        receiver,
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

    async fn connect(
        self: &Arc<Self>,
        url: &str,
        token: &str,
        options: SignalOptions,
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
    }

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

    // Wait for the reconnection task to finish
    // Return directly if no open RTCSession
    async fn wait_reconnection(&self) -> EngineResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            Err(EngineError::Connection("engine is closed".to_owned()))?
        }

        if self.reconnecting.load(Ordering::Acquire) {
            // If currently reconnecting, wait for the reconnect task to finish
            self.reconnect_notifier.notified().await;
        }

        // reconnect_task is finished here, so it is fine to try to read the RwLock here (should be a short lock)
        // (the reconnection logic can lock the running_handle for a long time, e.g when resuming)

        if self.running_handle.read().await.is_none() {
            Err(EngineError::Connection("reconnection failed".to_owned()))?
        }

        Ok(())
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
                    log::info!("RtcEngine successfully reconnected")
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
        let (url, token, options) = {
            let running_handle = self.running_handle.read().await;
            let signal_client = running_handle.as_ref().unwrap().session.signal_client();
            (
                signal_client.url(),
                signal_client.token(), // Refreshed token
                signal_client.options(),
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
                    .try_restart_connection(&url, &token, options.clone())
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
        options: SignalOptions,
    ) -> EngineResult<()> {
        self.terminate_session().await;
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
