use crate::signal_client::signal_stream::SignalStream;
use livekit_protocol as proto;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::RwLock as AsyncRwLock;
use tokio_tungstenite::tungstenite::Error as WsError;

mod signal_stream;

pub type SignalEmitter = mpsc::Sender<SignalEvent>;
pub type SignalEvents = mpsc::Receiver<SignalEvent>;
pub type SignalResult<T> = Result<T, SignalError>;

pub const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub const PROTOCOL_VERSION: u32 = 8;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("already connected")]
    AlreadyConnected,
    #[error("ws failure: {0}")]
    WsError(#[from] WsError),
    #[error("failed to parse the url")]
    UrlParse(#[from] url::ParseError),
    #[error("failed to decode messages from server")]
    ProtoParse(#[from] prost::DecodeError),
    #[error("{0}")]
    Timeout(String),
    #[error("failed to send message to server")]
    SendError,
}

/// Events used by the RTCEngine who will handle the reconnection logic
#[derive(Debug)]
pub enum SignalEvent {
    Open,
    Signal(proto::signal_response::Message),
    Close,
}

#[derive(Debug, Clone)]
pub struct SignalOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
}

impl Default for SignalOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
        }
    }
}

#[derive(Debug)]
pub struct SignalClient {
    stream: AsyncRwLock<Option<SignalStream>>,
    url: String,
    token: Mutex<String>, // TODO(theomonnom): Handle token refresh
    join_response: proto::JoinResponse,
    options: SignalOptions,
    emitter: SignalEmitter,
}

impl SignalClient {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(Self, proto::JoinResponse, SignalEvents)> {
        let (emitter, mut events) = mpsc::channel(8);
        let lk_url = get_livekit_url(url, token, &options)?;
        let new_stream = SignalStream::connect(lk_url, emitter.clone()).await?;
        let join_response = get_join_response(&mut events).await?;

        Ok((
            Self {
                stream: AsyncRwLock::new(Some(new_stream)),
                url: url.to_string(),
                token: Mutex::new(token.to_string()),
                join_response: join_response.clone(),
                options,
                emitter,
            },
            join_response,
            events,
        ))
    }

    // Restart is called when trying to resume the room (RtcSession resume)
    // TODO(theomonom): Should this be renamed to resume?
    pub async fn restart(&self) -> SignalResult<()> {
        self.close().await;

        let sid = &self.join_response.participant.as_ref().unwrap().sid;
        let token = self.token.lock().clone();

        let mut lk_url = get_livekit_url(&self.url, &token, &self.options)?;
        lk_url
            .query_pairs_mut()
            .append_pair("reconnect", "1")
            .append_pair("sid", sid);

        let new_stream = SignalStream::connect(lk_url, self.emitter.clone()).await?;
        *self.stream.write().await = Some(new_stream);
        Ok(())
    }

    pub async fn close(&self) {
        if let Some(stream) = self.stream.write().await.take() {
            stream.close().await;
        }
    }

    pub async fn send(&self, signal: proto::signal_request::Message) {
        // TODO: Check if currently reconnecting and queue message

        if let Some(stream) = self.stream.read().await.as_ref() {
            if stream.send(signal).await.is_ok() {
                return;
            }
        }
        // TODO(theomonnom): return result?
    }

    #[allow(dead_code)]
    pub async fn clear_queue(&self) {
        // TODO(theomonnom): Clear the queue
    }

    pub async fn flush_queue(&self) {
        // TODO(theomonnom): Send the queue
    }

    pub fn join_response(&self) -> proto::JoinResponse {
        self.join_response.clone()
    }

    pub fn options(&self) -> SignalOptions {
        self.options.clone()
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn token(&self) -> String {
        self.token.lock().clone()
    }
}

fn get_livekit_url(url: &str, token: &str, options: &SignalOptions) -> SignalResult<url::Url> {
    let mut lk_url = url::Url::parse(url)?;
    lk_url.set_path("/rtc");
    lk_url
        .query_pairs_mut()
        .append_pair("sdk", "rust")
        .append_pair("access_token", token)
        .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str())
        .append_pair(
            "auto_subscribe",
            if options.auto_subscribe { "1" } else { "0" },
        )
        .append_pair(
            "adaptive_stream",
            if options.adaptive_stream { "1" } else { "0" },
        );

    Ok(lk_url)
}

async fn get_join_response(receiver: &mut SignalEvents) -> SignalResult<proto::JoinResponse> {
    let join = async {
        while let Some(event) = receiver.recv().await {
            match event {
                SignalEvent::Signal(proto::signal_response::Message::Join(join)) => {
                    return Ok(join)
                }
                SignalEvent::Close => break,
                SignalEvent::Open => continue,
                _ => {
                    log::warn!(
                        "received unexpected message while waiting for JoinResponse: {:?}",
                        event
                    );
                    continue;
                }
            }
        }

        Err(WsError::ConnectionClosed)?
    };

    tokio::time::timeout(JOIN_RESPONSE_TIMEOUT, join)
        .await
        .map_err(|_| SignalError::Timeout("failed to receive JoinResponse".to_string()))?
}
