use livekit_webrtc::data_channel::DataSendError;
use livekit_webrtc::jsep::SdpParseError;
use livekit_webrtc::media_stream::MediaStream;
use livekit_webrtc::rtc_error::RTCError;
use livekit_webrtc::rtp_receiver::RtpReceiver;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Error;

use lazy_static::lazy_static;
use prost::Message;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tracing::{debug, error, info, trace, warn};

use livekit_webrtc::peer_connection_factory::RTCConfiguration;

use crate::proto::data_packet::Value;
use crate::proto::{
    data_packet, signal_request, signal_response, DataPacket, JoinResponse, ParticipantUpdate,
    SignalTarget, TrickleRequest,
};
use crate::rtc_engine::lk_runtime::LKRuntime;
use crate::rtc_engine::pc_transport::PCTransport;
use crate::rtc_engine::rtc_events::{RTCEmitter, RTCEvent, RTCEvents};
use crate::signal_client::{SignalClient, SignalError, SignalEvent, SignalEvents, SignalOptions};

use self::rtc_session::{RTCSession, SessionEvent, SessionEvents};

mod lk_runtime;
mod pc_transport;
mod rtc_events;
mod rtc_session;

pub(crate) type EngineEmitter = mpsc::Sender<EngineEvent>;
pub(crate) type EngineEvents = mpsc::Receiver<EngineEvent>;
pub(crate) type EngineResult<T> = Result<T, EngineError>;

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
    AddTrack {
        rtp_receiver: RtpReceiver,
        streams: Vec<MediaStream>,
    },
    Connected,
    Resuming,
    Resumed,
    SignalResumed,
    Restarting,
    Restarted,
}

//
// TODO(theomonnom): Smarter retry intervals
pub(crate) const RECONNECT_ATTEMPTS: u32 = 10;
pub(crate) const RECONNECT_INTERVAL: Duration = Duration::from_millis(300);

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

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
    running_handle: RwLock<Option<EngineHandle>>,
    reconnecting: AtomicBool,
    opened: AtomicBool,
    engine_emitter: EngineEmitter,
}

#[derive(Debug)]
pub struct RTCEngine {
    lk_runtime: Arc<LKRuntime>,
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
            running_handle: Default::default(),
            reconnecting: Default::default(),
            opened: Default::default(),
            engine_emitter,
        });

        (
            Self {
                lk_runtime: lk_runtime.unwrap(),
                inner,
            },
            engine_events,
        )
    }

    #[tracing::instrument]
    pub async fn connect(
        &self,
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<()> {
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
        let engine_task = tokio::spawn(
            self.inner
                .clone()
                .engine_task(session_events, close_receiver),
        );

        self.inner.opened.store(true, Ordering::SeqCst);
        *self.inner.running_handle.write() = Some(EngineHandle {
            session,
            engine_task,
            close_sender,
        });

        Ok(())
    }

    #[tracing::instrument]
    pub async fn close(&self) {
        self.inner.opened.store(false, Ordering::SeqCst);
        self.inner.close();
    }

    #[tracing::instrument(skip(data))]
    pub async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.inner.wait_reconnection().await?;
        self.inner
            .running_handle
            .read()
            .as_ref()
            .unwrap()
            .session
            .publish_data(data, kind)
            .await?;

        Ok(())
    }

    pub fn join_response(&self) -> Option<JoinResponse> {
        if let Some(handle) = self.inner.running_handle.read().as_ref() {
            Some(handle.session.info().join_response.clone())
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

    async fn on_session_event(&self, event: SessionEvent) -> EngineResult<()> {
        Ok(())
    }

    async fn close(&self) {
        if let Some(handle) = self.running_handle.write().take() {
            handle.session.close().await;
            let _ = handle.close_sender.send(());
            handle.engine_task.await;
        }
    }

    async fn wait_reconnection(&self) -> EngineResult<()> {
        if !self.opened.load(Ordering::SeqCst) {
            Err(EngineError::Connection("not opened".to_owned()))?
        }

        while self.reconnecting.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }

        if self.running_handle.read().is_none() {
            Err(EngineError::Connection("reconnection failed".to_owned()))?
        }

        Ok(())
    }

    /// Called every time the PeerConnection or the SignalClient is closed
    /// We first try to resume the connection, if it fails, we start a full reconnect.
    async fn handle_disconnected(&self) {
        if !self.opened.load(Ordering::SeqCst) || self.reconnecting.load(Ordering::SeqCst) {
            return;
        }

        self.reconnecting.store(true, Ordering::SeqCst);
        warn!("RTCEngine disconnected unexpectedly, reconnecting...");

        let mut full_reconnect = false;
        for i in 0..RECONNECT_ATTEMPTS {
            if full_reconnect {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Restarting).await;
                }

                info!("restarting connection... attempt: {}", i);
                if let Err(err) = self.try_restart_connection().await {
                    error!("restarting connection failed: {}", err);
                } else {
                    let _ = self.engine_emitter.send(EngineEvent::Restarted).await;
                    return;
                }
            } else {
                if i == 0 {
                    let _ = self.engine_emitter.send(EngineEvent::Resuming).await;
                }

                info!("resuming connection... attempt: {}", i);
                if let Err(err) = self.try_resume_connection().await {
                    error!("resuming connection failed: {}", err);
                    if let EngineError::Signal(_) = err {
                        full_reconnect = true;
                    }
                } else {
                    let _ = self.engine_emitter.send(EngineEvent::Resumed).await;
                    return;
                }
            }

            tokio::time::sleep(RECONNECT_INTERVAL).await;
        }
        error!("failed to reconnect after {} attemps", RECONNECT_ATTEMPTS);
        self.reconnecting.store(false, Ordering::SeqCst);

        // TODO DISCONNECT
    }

    /// Try to recover the connection by doing a full reconnect.
    /// It creates a new RTCSession
    async fn try_restart_connection(&self) -> EngineResult<()> {
        Ok(())
    }

    /// Try to restart the current session
    async fn try_resume_connection(&self) -> EngineResult<()> {
        Ok(())
    }
}
