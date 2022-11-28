use std::fmt::Debug;
use std::time::Duration;

use livekit_webrtc::peer_connection_factory::{
    ContinualGatheringPolicy, ICEServer, IceTransportsType, RTCConfiguration,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Error as WsError;

use crate::proto::{signal_request, signal_response, JoinResponse};
use crate::signal_client::signal_stream::SignalStream;

mod signal_stream;

pub(crate) type SignalEmitter = mpsc::Sender<SignalEvent>;
pub(crate) type SignalEvents = mpsc::Receiver<SignalEvent>;
pub(crate) type SignalResult<T> = Result<T, SignalError>;

pub const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("websocket failure")]
    WsError(#[from] WsError),
    #[error("failed to parse the url")]
    UrlParse(#[from] url::ParseError),
    #[error("failed to decode messages from server")]
    ProtoParse(#[from] prost::DecodeError),
    #[error("{0}")]
    Timeout(String),
}

/// Events used by the RTCEngine who will handle the reconnection logic
#[derive(Debug)]
pub(crate) enum SignalEvent {
    Open,
    Signal(signal_response::Message),
    Close,
}

#[derive(Debug)]
pub(crate) struct SignalOptions {
    reconnect: bool,
    auto_subscribe: bool,
    sid: String,
    adaptive_stream: bool,
}

impl Default for SignalOptions {
    fn default() -> Self {
        Self {
            reconnect: false,
            auto_subscribe: true,
            sid: "".to_string(),
            adaptive_stream: false,
        }
    }
}

#[derive(Debug)]
pub struct SignalClient {
    stream: SignalStream,
    emitter: SignalEmitter,
}

impl SignalClient {
    pub(crate) async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(Self, SignalEvents)> {
        let (emitter, events) = mpsc::channel(8);
        let stream = SignalStream::connect(url, token, options, emitter.clone()).await?;

        // TODO(theomonnom) Retry initial connection

        Ok((Self { stream, emitter }, events))
    }

    pub async fn send(&self, signal: signal_request::Message) {
        if let Err(_) = self.stream.send(signal).await {
            // TODO(theomonnom) Queue message ( Ignore on full reconnect )
        }
    }

    pub async fn reconnect(&self) {
        // TODO(theomonnom) Close & recreate SignalStream, also send the queue if needed
    }
}

impl From<JoinResponse> for RTCConfiguration {
    fn from(join_response: JoinResponse) -> Self {
        Self {
            ice_servers: {
                let mut servers = vec![];
                for ice_server in join_response.ice_servers.clone() {
                    servers.push(ICEServer {
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
}

pub mod utils {
    use crate::proto::{signal_response, JoinResponse};
    use crate::signal_client::{SignalError, SignalEvent, SignalResult, JOIN_RESPONSE_TIMEOUT};
    use tokio::sync::mpsc;
    use tokio::time::timeout;
    use tokio_tungstenite::tungstenite::Error as WsError;
    use tracing::{event, Level};

    pub(crate) async fn next_join_response(
        receiver: &mut mpsc::Receiver<SignalEvent>,
    ) -> SignalResult<JoinResponse> {
        let join = async {
            while let Some(event) = receiver.recv().await {
                match event {
                    SignalEvent::Signal(signal_response::Message::Join(join)) => return Ok(join),
                    SignalEvent::Close => break,
                    SignalEvent::Open => continue,
                    _ => {
                        event!(
                            Level::WARN,
                            "received unexpected message while waiting for JoinResponse: {:?}",
                            event
                        );
                        continue;
                    }
                }
            }

            Err(WsError::ConnectionClosed)?
        };

        timeout(JOIN_RESPONSE_TIMEOUT, join)
            .await
            .map_err(|_| SignalError::Timeout("failed to receive JoinResponse".to_string()))?
    }
}
