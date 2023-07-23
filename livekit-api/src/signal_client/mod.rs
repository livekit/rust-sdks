use crate::signal_client::signal_stream::SignalStream;
use livekit_protocol as proto;
use parking_lot::Mutex;
use reqwest::StatusCode;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::RwLock as AsyncRwLock;
use tokio_tungstenite::tungstenite::Error as WsError;

mod signal_stream;

pub type SignalEmitter = mpsc::UnboundedSender<SignalEvent>;
pub type SignalEvents = mpsc::UnboundedReceiver<SignalEvent>;
pub type SignalResult<T> = Result<T, SignalError>;

pub const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub const PROTOCOL_VERSION: u32 = 8;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("already connected")]
    AlreadyConnected,
    #[error("already reconnecting")]
    AlreadyReconnecting,
    #[error("ws failure: {0}")]
    WsError(#[from] WsError),
    #[error("failed to parse the url {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("client error: {0} - {1}")]
    Client(StatusCode, String),
    #[error("server error: {0} - {1}")]
    Server(StatusCode, String),
    #[error("failed to decode messages from server: {0}")]
    ProtoParse(#[from] prost::DecodeError),
    #[error("{0}")]
    Timeout(String),
    #[error("failed to send message to server")]
    SendError,
}

/// Events used by the RtcSession who will handle the reconnection logic
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

struct SignalInner {
    stream: AsyncRwLock<Option<SignalStream>>,
    token: Mutex<String>, // Token can be refreshed
}

pub struct SignalClient {
    inner: Arc<SignalInner>,
    emitter: SignalEmitter,
    reconnecting: AtomicBool,

    url: String,
    options: SignalOptions,
    join_response: proto::JoinResponse,
}

impl Debug for SignalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalClient")
            .field("url", &self.url)
            .field("reconnecting", &self.reconnecting)
            .field("join_response", &self.join_response)
            .field("options", &self.options)
            .finish()
    }
}

impl SignalClient {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(Self, proto::JoinResponse, SignalEvents)> {
        let (internal_emitter, mut internal_events) = mpsc::unbounded_channel();
        let lk_url = get_livekit_url(url, token, &options)?;

        // Try to connect to the SignalClient
        let stream_res = SignalStream::connect(lk_url.clone(), internal_emitter.clone()).await;
        if let Err(err) = stream_res {
            // Connection failed, try to retrieve more informations
            if let SignalError::WsError(WsError::Http(_)) = err {
                Self::validate(lk_url).await?;
            }

            return Err(err);
        }

        // Successfully connected to the SignalClient
        let join_response = get_join_response(&mut internal_events).await?;
        let inner = Arc::new(SignalInner {
            stream: AsyncRwLock::new(Some(stream_res.unwrap())),
            token: Mutex::new(token.to_owned()),
        });

        let (emitter, events) = mpsc::unbounded_channel();
        tokio::spawn(Self::signal_task(inner.clone(), emitter, internal_events));

        let client = Self {
            inner,
            emitter: internal_emitter,
            reconnecting: AtomicBool::new(false),
            options,
            url: url.to_string(),
            join_response: join_response.clone(),
        };

        Ok((client, join_response, events))
    }

    /// Validate the connection by calling rtc/validate
    async fn validate(mut ws_url: url::Url) -> SignalResult<()> {
        ws_url
            .set_scheme(if ws_url.scheme() == "wss" {
                "https"
            } else {
                "http"
            })
            .unwrap();
        ws_url.set_path("/rtc/validate");

        if let Ok(res) = reqwest::get(ws_url.as_str()).await {
            let status = res.status();
            let body = res.text().await.ok().unwrap_or_default();

            if status.is_client_error() {
                return Err(SignalError::Client(status, body));
            } else if status.is_server_error() {
                return Err(SignalError::Server(status, body));
            }
        }

        Ok(())
    }

    /// Middleware task to receive SignalStream events and handle SignalClient specific logic
    async fn signal_task(
        inner: Arc<SignalInner>,
        emitter: SignalEmitter, // Public emitter
        mut internal_events: SignalEvents,
    ) {
        while let Some(event) = internal_events.recv().await {
            if let SignalEvent::Signal(ref event) = event {
                match event {
                    proto::signal_response::Message::RefreshToken(ref token) => {
                        // Refresh the token so the client can still reconnect if the initial join token expired
                        *inner.token.lock() = token.clone();
                    }
                    _ => {}
                }
            }

            let _ = emitter.send(event);
        }
    }

    /// Restart is called when trying to resume the room (RtcSession resume)
    pub async fn restart(&self) -> SignalResult<()> {
        self.reconnecting.store(true, Ordering::Release);

        self.close().await;
        let mut stream = self.inner.stream.write().await;

        let sid = &self.join_response.participant.as_ref().unwrap().sid;
        let token = self.inner.token.lock().clone();

        let mut lk_url = get_livekit_url(&self.url, &token, &self.options).unwrap();
        lk_url
            .query_pairs_mut()
            .append_pair("reconnect", "1")
            .append_pair("sid", sid);

        let res = SignalStream::connect(lk_url, self.emitter.clone()).await;
        match res {
            Ok(new_stream) => {
                *stream = Some(new_stream);
                self.reconnecting.store(false, Ordering::Release);
                Ok(())
            }
            Err(err) => {
                self.reconnecting.store(false, Ordering::Release);
                Err(err)
            }
        }
    }

    /// Close the connection
    pub async fn close(&self) {
        let mut stream = self.inner.stream.write().await;
        if let Some(stream) = stream.take() {
            stream.close().await;
        }
    }

    /// Send a signal to the server
    pub async fn send(&self, signal: proto::signal_request::Message) {
        if self.reconnecting.load(Ordering::Acquire) {
            if is_queuable(&signal) {
                // Push to queue
            }

            return;
        }

        let stream = self.inner.stream.read().await;
        if let Some(stream) = stream.as_ref() {
            if let Err(err) = stream.send(signal).await {
                log::error!("failed to send signal: {}", err);
            }
        }
    }

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

    /// Returns the last refreshed token (Or initial token if not refreshed yet)
    pub fn token(&self) -> String {
        self.inner.token.lock().clone()
    }
}

/// Check if the signal is queuable
/// Not every signal should be sent after signal reconnection
fn is_queuable(signal: &proto::signal_request::Message) -> bool {
    return matches!(
        signal,
        proto::signal_request::Message::SyncState(_)
            | proto::signal_request::Message::Trickle(_)
            | proto::signal_request::Message::Offer(_)
            | proto::signal_request::Message::Answer(_)
            | proto::signal_request::Message::Simulate(_)
            | proto::signal_request::Message::Leave(_)
    );
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
