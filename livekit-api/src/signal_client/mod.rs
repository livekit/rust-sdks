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
use tokio::sync::Mutex as AsyncMutex;
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

pub enum SignalEvent {
    Message(Box<proto::signal_response::Message>),
    Close, // Need restart
}

struct SignalInner {
    stream: AsyncRwLock<Option<SignalStream>>,
    token: Mutex<String>, // Token can be refreshed
}

pub struct SignalClient {
    inner: Arc<SignalInner>,
    emitter: SignalEmitter,
    reconnecting: AtomicBool,
    queue: AsyncMutex<Vec<proto::signal_request::Message>>,

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
        let lk_url = get_livekit_url(url, token, &options)?;

        // Try to connect to the SignalClient
        let (stream, mut stream_events) = match SignalStream::connect(lk_url.clone()).await {
            Ok(stream) => stream,
            Err(err) => {
                // Connection failed, try to retrieve more informations
                Self::validate(lk_url).await?;
                return Err(err);
            }
        };

        // Successfully connected to the SignalClient
        let inner = Arc::new(SignalInner {
            stream: AsyncRwLock::new(Some(stream)),
            token: Mutex::new(token.to_owned()),
        });

        let join_response = get_join_response(&mut stream_events).await?;
        let (emitter, events) = mpsc::unbounded_channel();
        tokio::spawn(signal_task(inner.clone(), emitter.clone(), stream_events));

        let client = Self {
            inner,
            emitter,
            reconnecting: AtomicBool::new(false),
            queue: Default::default(),
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

    /// Restart is called when trying to resume the room (RtcSession resume)
    pub async fn restart(&self) -> SignalResult<proto::ReconnectResponse> {
        self.reconnecting.store(true, Ordering::Release);
        scopeguard::defer!(self.reconnecting.store(false, Ordering::Release));

        self.close().await;

        // Lock while we are reconnecting
        let mut stream = self.inner.stream.write().await;
        let sid = &self.join_response.participant.as_ref().unwrap().sid;
        let token = self.inner.token.lock().clone();

        let mut lk_url = get_livekit_url(&self.url, &token, &self.options).unwrap();
        lk_url
            .query_pairs_mut()
            .append_pair("reconnect", "1")
            .append_pair("sid", sid);

        let (new_stream, mut signal_events) = SignalStream::connect(lk_url).await?;
        let reconnect_response = get_reconnect_response(&mut signal_events).await?;
        tokio::spawn(signal_task(
            self.inner.clone(),
            self.emitter.clone(),
            signal_events,
        ));

        *stream = Some(new_stream);
        drop(stream);
        self.flush_queue().await;

        Ok(reconnect_response)
    }

    /// Close the connection
    pub async fn close(&self) {
        if let Some(stream) = self.inner.stream.write().await.take() {
            stream.close().await;
        }
    }

    /// Send a signal to the server
    pub async fn send(&self, signal: proto::signal_request::Message) {
        if self.reconnecting.load(Ordering::Acquire) {
            self.queue_message(signal).await;
            return;
        }

        self.flush_queue().await; // The queue must be flusehd before sending any new signal

        if let Some(stream) = self.inner.stream.read().await.as_ref() {
            if stream.send(signal.clone()).await.is_err() {
                self.queue_message(signal).await;
            }
        }
    }

    async fn queue_message(&self, signal: proto::signal_request::Message) {
        if is_queuable(&signal) {
            self.queue.lock().await.push(signal);
        }
    }

    pub async fn flush_queue(&self) {
        let mut queue = self.queue.lock().await;
        if queue.is_empty() {
            return;
        }

        if let Some(stream) = self.inner.stream.read().await.as_ref() {
            for signal in queue.drain(..) {
                log::warn!("sending queued signal: {:?}", signal);

                if let Err(err) = stream.send(signal).await {
                    log::error!("failed to send queued signal: {}", err); // Lost message
                }
            }
        }
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

/// Middleware task to receive SignalStream events and handle SignalClient specific logic
/// TODO(theomonnom): should we use tokio_stream?
async fn signal_task(
    inner: Arc<SignalInner>,
    emitter: SignalEmitter, // Public emitter
    mut internal_events: mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
) {
    while let Some(signal) = internal_events.recv().await {
        if let proto::signal_response::Message::RefreshToken(ref token) = signal.as_ref() {
            *inner.token.lock() = token.clone(); // Refresh the token so the client can still reconnect if the initial join token expired
        }

        // TODO(theomonnom): should we handle signal ping pong on native side?
        let _ = emitter.send(SignalEvent::Message(signal));
    }

    // internal_events is closed, send an event to notify the close
    let _ = emitter.send(SignalEvent::Close);
}

/// Check if the signal is queuable
/// Not every signal should be sent after signal reconnection
fn is_queuable(signal: &proto::signal_request::Message) -> bool {
    matches!(
        signal,
        proto::signal_request::Message::SyncState(_)
            | proto::signal_request::Message::Trickle(_)
            | proto::signal_request::Message::Offer(_)
            | proto::signal_request::Message::Answer(_)
            | proto::signal_request::Message::Simulate(_)
            | proto::signal_request::Message::Leave(_)
    )
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

macro_rules! get_async_message {
    ($fnc:ident, $pattern:pat => $result:expr, $ty:ty) => {
        async fn $fnc(
            receiver: &mut mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
        ) -> SignalResult<$ty> {
            let join = async {
                while let Some(event) = receiver.recv().await {
                    if let $pattern = *event {
                        return Ok($result);
                    }
                }

                Err(WsError::ConnectionClosed)?
            };

            tokio::time::timeout(JOIN_RESPONSE_TIMEOUT, join)
                .await
                .map_err(|_| {
                    SignalError::Timeout(format!(
                        "failed to receive {}",
                        std::any::type_name::<$ty>()
                    ))
                })?
        }
    };
}

get_async_message!(
    get_join_response,
    proto::signal_response::Message::Join(msg) => msg,
    proto::JoinResponse
);

get_async_message!(
    get_reconnect_response,
    proto::signal_response::Message::Reconnect(msg) => msg,
    proto::ReconnectResponse
);
