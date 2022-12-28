use futures::future::BoxFuture;
use futures::FutureExt;
use livekit_webrtc::data_channel::DataSendError;
use livekit_webrtc::jsep::SdpParseError;
use livekit_webrtc::media_stream::{MediaStream, MediaStreamTrackHandle};
use livekit_webrtc::rtc_error::RTCError;
use livekit_webrtc::rtp_receiver::RtpReceiver;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock as AsyncRwLock;
use tokio::task::JoinHandle;
use tokio::time::{interval, Interval};

use lazy_static::lazy_static;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

use crate::proto::{data_packet, DataPacket, JoinResponse, ParticipantUpdate, SpeakerInfo};
use crate::rtc_engine::lk_runtime::LKRuntime;
use crate::signal_client::{SignalError, SignalOptions};

use self::rtc_session::{RTCSession, SessionEvent, SessionEvents, SessionInfo};

mod lk_runtime;
mod pc_transport;
mod rtc_events;
mod rtc_session;

pub(crate) type EngineEmitter = mpsc::Sender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::Receiver<EngineEvent>;
pub(crate) type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Clone, Eq, PartialEq)]
#[repr(u8)]
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
pub enum EngineEvent {
    ParticipantUpdate(ParticipantUpdate),
    MediaTrack {
        track: MediaStreamTrackHandle,
        stream: MediaStream,
        receiver: RtpReceiver,
    },
    Data {
        participant_sid: String,
        payload: Vec<u8>,
        kind: data_packet::Kind,
    },
    SpeakersChanged {
        speakers: Vec<SpeakerInfo>,
    },
    Resuming,
    Resumed,
    Restarting,
    Restarted,
    Disconnected,
}

pub const RECONNECT_ATTEMPTS: u32 = 10;
pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}
///
/// Represents a running RTCSession with the ability to close the session
/// and the engine_task
#[derive(Debug)]
struct EngineHandle {
    session: RTCSession,
    engine_task: JoinHandle<()>,
    close_sender: oneshot::Sender<()>,
}

#[derive(Debug)]
struct EngineInner {
    lk_runtime: Arc<LKRuntime>,
    session_info: Mutex<Option<SessionInfo>>, // Last/Current Sessioninfo
    running_handle: AsyncRwLock<Option<EngineHandle>>,
    opened: AtomicBool,
    engine_emitter: EngineEmitter,

    // Reconnecting fields
    reconnecting: AtomicBool,
    full_reconnect: AtomicBool,
    reconnect_interval: Mutex<Interval>,
}

#[derive(Debug)]
pub struct RTCEngine {
    inner: Arc<EngineInner>,
}

impl RTCEngine {
    pub fn new() -> (Self, EngineEvents) {
        let mut lk_runtime = None;
        {
            let mut lk_runtime_ref = LK_RUNTIME.lock();
            lk_runtime = lk_runtime_ref.upgrade();

            if lk_runtime.is_none() {
                let new_runtime = Arc::new(LKRuntime::default());
                *lk_runtime_ref = Arc::downgrade(&new_runtime);
                lk_runtime = Some(new_runtime);
            }
        }

        let (engine_emitter, engine_events) = mpsc::channel(8);
        let inner = Arc::new(EngineInner {
            lk_runtime: lk_runtime.unwrap(),
            session_info: Default::default(),
            running_handle: Default::default(),
            opened: Default::default(),
            engine_emitter,
            reconnecting: Default::default(),
            full_reconnect: Default::default(),
            reconnect_interval: Mutex::new(interval(RECONNECT_INTERVAL)),
        });

        (Self { inner }, engine_events)
    }

    #[tracing::instrument]
    pub async fn connect(
        &self,
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<()> {
        self.inner.connect(url, token, options).await
    }

    #[tracing::instrument]
    pub async fn close(&self) {
        self.inner.close().await
    }

    #[tracing::instrument(skip(data))]
    pub async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> EngineResult<()> {
        self.inner.wait_reconnection().await?;
        self.inner
            .running_handle
            .read()
            .await
            .as_ref()
            .unwrap()
            .session
            .publish_data(data, kind)
            .await
    }

    pub async fn simulate_scenario(&self, scenario: SimulateScenario) -> EngineResult<()> {
        self.inner.wait_reconnection().await?;
        self.inner
            .running_handle
            .read()
            .await
            .as_ref()
            .unwrap()
            .session
            .simulate_scenario(scenario)
            .await;
        Ok(())
    }

    pub fn join_response(&self) -> Option<JoinResponse> {
        if let Some(info) = self.inner.session_info.lock().as_ref() {
            Some(info.join_response.clone())
        } else {
            None
        }
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
                            error!("failed to handle session event: {:?}", err);
                        }
                    } else {
                        panic!("rtc_sessions has been closed unexpectedly");
                    }
                },
                 _ = &mut close_receiver => {
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
                info!("received session close: {}, {:?}", source, reason);
                if can_reconnect {
                    self.clone().try_reconnect(retry_now, full_reconnect);
                } else {
                    self.close().await;
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
            } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::MediaTrack {
                        track,
                        stream,
                        receiver,
                    })
                    .await;
            }
            SessionEvent::SpeakersChanged { speakers } => {
                let _ = self
                    .engine_emitter
                    .send(EngineEvent::SpeakersChanged { speakers })
                    .await;
            }
            SessionEvent::Connected => {}
        }
        Ok(())
    }

    fn connect<'a>(
        self: &'a Arc<Self>,
        url: &'a str,
        token: &'a str,
        options: SignalOptions,
    ) -> BoxFuture<'a, EngineResult<()>> {
        async {
            let (session_emitter, session_events) = mpsc::unbounded_channel();
            let session = RTCSession::connect(
                url,
                token,
                options,
                self.lk_runtime.clone(),
                session_emitter,
            )
            .await?;

            let (close_sender, close_receiver) = oneshot::channel();
            let engine_task =
                tokio::spawn(self.clone().engine_task(session_events, close_receiver));

            *self.session_info.lock() = Some(session.info().clone());
            *self.running_handle.write().await = Some(EngineHandle {
                session,
                engine_task,
                close_sender,
            });

            self.opened.store(true, Ordering::SeqCst);
            Ok(())
        }
        .boxed()
    }

    async fn terminate_session(&self) {
        if let Some(handle) = self.running_handle.write().await.take() {
            handle.session.close().await;
            let _ = handle.close_sender.send(());
            let _ = handle.engine_task.await;
        }
    }

    async fn close(&self) {
        self.opened.store(false, Ordering::SeqCst);
        self.terminate_session().await;
        let _ = self.engine_emitter.send(EngineEvent::Disconnected).await;
    }

    // Wait for the reconnection task to finish
    // Return directly if no open RTCSession
    async fn wait_reconnection(&self) -> EngineResult<()> {
        if !self.opened.load(Ordering::SeqCst) {
            Err(EngineError::Connection("not opened".to_owned()))?
        }

        while self.reconnecting.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }

        if self.running_handle.read().await.is_none() {
            Err(EngineError::Connection("reconnection failed".to_owned()))?
        }

        Ok(())
    }

    /// Start the reconnect task if not already started
    fn try_reconnect(self: Arc<Self>, retry_now: bool, full_reconnect: bool) {
        if !self.opened.load(Ordering::SeqCst) {
            return;
        }

        if self.reconnecting.load(Ordering::SeqCst) {
            if retry_now {
                self.reconnect_interval.lock().reset();
                self.full_reconnect.store(full_reconnect, Ordering::SeqCst);
            }
            return;
        }

        warn!("reconnecting RTCEngine...");

        self.reconnecting.store(true, Ordering::SeqCst);
        self.full_reconnect.store(full_reconnect, Ordering::SeqCst);
        self.reconnect_interval.lock().reset();
        tokio::spawn({
            let inner = self.clone();
            async move {
                let res = inner.reconnect_task().await;
                inner.reconnecting.store(false, Ordering::SeqCst);

                if res.is_ok() {
                    warn!("RTCEngine successfully reconnected")
                } else {
                    error!("failed to reconnect after {} attemps", RECONNECT_ATTEMPTS);
                    inner.close().await;
                }
            }
        });
    }

    /// Called every time the PeerConnection or the SignalClient is closed
    /// We first try to resume the connection, if it fails, we start a full reconnect.
    async fn reconnect_task(self: &Arc<Self>) -> EngineResult<()> {
        for i in 0..RECONNECT_ATTEMPTS {
            if !self.opened.load(Ordering::Acquire) {
                // The user closed the RTCEngine, cancel the reconnection task
                return Ok(());
            }

            if self.full_reconnect.load(Ordering::SeqCst) {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Restarting).await;
                }

                info!("restarting connection... attempt: {}", i);
                if let Err(err) = self.try_restart_connection().await {
                    error!("restarting connection failed: {}", err);
                } else {
                    let _ = self.engine_emitter.send(EngineEvent::Restarted).await;
                    return Ok(());
                }
            } else {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Resuming).await;
                }

                info!("resuming connection... attempt: {}", i);
                if let Err(err) = self.try_resume_connection().await {
                    error!("resuming connection failed: {}", err);
                    if let EngineError::Signal(_) = err {
                        self.full_reconnect.store(true, Ordering::SeqCst);
                    }
                } else {
                    let _ = self.engine_emitter.send(EngineEvent::Resumed).await;
                    return Ok(());
                }
            }

            self.reconnect_interval.lock().tick().await;
        }

        Err(EngineError::Connection("failed to reconnect".to_owned()))
    }

    /// Try to recover the connection by doing a full reconnect.
    /// It recreates a new RTCSession
    async fn try_restart_connection(self: &Arc<Self>) -> EngineResult<()> {
        let info = self.session_info.lock().clone().unwrap();
        self.terminate_session().await;
        self.connect(&info.url, &info.token, info.options).await?;
        self.running_handle
            .read()
            .await
            .as_ref()
            .unwrap()
            .session
            .wait_pc_connection()
            .await

        // TODO(theomonnom): Resend SignalClient queue
    }

    /// Try to restart the current session
    async fn try_resume_connection(&self) -> EngineResult<()> {
        let handle = self.running_handle.read().await;
        handle.as_ref().unwrap().session.restart().await?;
        handle.as_ref().unwrap().session.wait_pc_connection().await
    }
}
