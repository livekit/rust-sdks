use crate::lk_runtime::LKRuntime;
use crate::proto::{data_packet, signal_response, DataPacket, JoinResponse, UserPacket};
use crate::rtc_engine::engine_internal::EngineInternal;
use crate::signal_client::{SignalClient, SignalError, SignalEvent, SignalOptions};
use futures_util::{FutureExt, StreamExt};
use lazy_static::lazy_static;
use livekit_webrtc::data_channel::DataSendError;
use livekit_webrtc::jsep::SdpParseError;
use livekit_webrtc::rtc_error::RTCError;
use prost::Message;
use std::sync::{Arc, Weak};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::{event, Level};

mod engine_internal;

lazy_static! {
    // Share one LKRuntime across all RTCEngine instances
    static ref LK_RUNTIME: Mutex<Weak<LKRuntime>> = Mutex::new(Weak::new());
}

pub(crate) const MAX_ICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

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
pub struct Packet {
    pub data: UserPacket,
    pub kind: data_packet::Kind,
}

#[derive(Debug)]
pub enum EngineEvent {
    DataReceived(Packet),
}

#[derive(Debug)]
pub struct RTCEngine {
    signal_client: Arc<SignalClient>,
    internal: Arc<EngineInternal>,

    #[allow(unused)]
    lk_runtime: Arc<LKRuntime>, // Keep a reference while we're using the RTCEngine
}

#[tracing::instrument(skip(url, token))]
pub async fn connect(
    url: &str,
    token: &str,
    options: SignalOptions,
) -> Result<RTCEngine, EngineError> {
    // Acquire an existing/a new LKRuntime
    let mut lk_runtime_ref = LK_RUNTIME.lock().await;
    let mut lk_runtime = lk_runtime_ref.upgrade();

    if lk_runtime.is_none() {
        let new_runtime = Arc::new(LKRuntime::new());
        *lk_runtime_ref = Arc::downgrade(&new_runtime);
        lk_runtime = Some(new_runtime);
    }
    let lk_runtime = lk_runtime.unwrap();
    let (signal_client, mut signal_events) = SignalClient::connect(url, token, options).await?;
    let signal_client = Arc::new(signal_client);

    let join_response = time::timeout(JOIN_RESPONSE_TIMEOUT, async move {
        while let Some(event) = signal_events.next().await {
            match event {
                SignalEvent::Signal(signal_response::Message::Join(join)) => return join,
                _ => {
                    // Should we try a reconnect on close here?
                    continue;
                }
            }
        }

        unreachable!();
    })
    .await
    .map_err(|_| EngineError::Internal("failed to receive JoinResponse".to_string()))?;

    event!(Level::DEBUG, "received JoinResponse: {:?}", join_response);

    let (sender, receiver) = mpsc::channel(8);
    let internal = Arc::new(EngineInternal::configure(
        lk_runtime.clone(),
        sender,
        join_response.clone(),
    )?);

    if !join_response.subscriber_primary {
        internal.publisher_pc.lock().await.negotiate().await?;
    }

    tokio::spawn({
        let signal_client = signal_client.clone();
        let internal = internal.clone();

        async move {
            internal.run(receiver, signal_client).await;
        }
    });

    Ok(RTCEngine {
        lk_runtime,
        signal_client,
        internal,
    })
}

impl RTCEngine {
    /// Send data to other participants in the Room
    #[tracing::instrument]
    pub async fn publish_data(
        &mut self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.internal.ensure_publisher_connected(kind).await?;
        self.internal
            .data_channel(kind)
            .lock()
            .await
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    /// Return the last received JoinResponse
    pub async fn join_response(&self) -> JoinResponse {
        self.internal.join_response.lock().await.clone()
    }
}
