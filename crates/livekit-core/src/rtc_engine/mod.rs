use parking_lot::Mutex;
use std::error;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::sync::{mpsc, Mutex as AsyncMutex};

use lazy_static::lazy_static;
use prost::Message;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{debug, error, info, trace, warn};

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

mod engine_internal;
mod lk_runtime;
mod pc_transport;
mod rtc_session;
mod rtc_events;

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

#[derive(Debug)]
pub struct RTCEngine {
    engine_inner: Arc<EngineInternal>,
}

impl RTCEngine {
    pub fn new() -> Self {
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

        let (signal_client, mut signal_events) = SignalClient::new();

        Self { lk_runtime }
    }

    #[tracing::instrument(skip(url, token))]
    pub(crate) async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> EngineResult<(RTCEngine, EngineEvents)> {
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

        if !join_response.subscriber_primary {
            engine_inner.negotiate_publisher().await?;
        }

        let rtc_engine = Self {
            signal_client,
            engine_inner,
            lk_runtime,
        };

        Ok((rtc_engine, events))
    }

    #[tracing::instrument]
    pub async fn publish_data(
        &self,
        data: &DataPacket,
        kind: data_packet::Kind,
    ) -> Result<(), EngineError> {
        self.engine_inner.ensure_publisher_connected(kind).await?;
        self.engine_inner
            .data_channel(kind)
            .lock()
            .send(&data.encode_to_vec(), true)
            .map_err(Into::into)
    }

    pub fn join_response(&self) -> JoinResponse {
        self.engine_inner.join_response.lock().clone()
    }

    fn close(&self) {
        // TODO
    }
}
