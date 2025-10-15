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

use std::{
    borrow::Cow,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use http::StatusCode;
use livekit_protocol as proto;
use livekit_runtime::{interval, sleep, Instant, JoinHandle};
use parking_lot::Mutex;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock as AsyncRwLock};

#[cfg(feature = "signal-client-tokio")]
use tokio_tungstenite::tungstenite::Error as WsError;

#[cfg(feature = "__signal-client-async-compatible")]
use async_tungstenite::tungstenite::Error as WsError;

use crate::{http_client, signal_client::signal_stream::SignalStream};

mod region;
mod signal_stream;

pub use region::RegionUrlProvider;

pub type SignalEmitter = mpsc::UnboundedSender<SignalEvent>;
pub type SignalEvents = mpsc::UnboundedReceiver<SignalEvent>;
pub type SignalResult<T> = Result<T, SignalError>;

pub const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub const PROTOCOL_VERSION: u32 = 16;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("ws failure: {0}")]
    WsError(#[from] WsError),
    #[error("failed to parse the url: {0}")]
    UrlParse(String),
    #[error("access token has invalid characters")]
    TokenFormat,
    #[error("client error: {0} - {1}")]
    Client(StatusCode, String),
    #[error("server error: {0} - {1}")]
    Server(StatusCode, String),
    #[error("failed to decode messages from server: {0}")]
    ProtoParse(#[from] prost::DecodeError),
    #[error("{0}")]
    Timeout(String),
    #[error("failed to send message to the server")]
    SendError,
    #[error("failed to retrieve region info: {0}")]
    RegionError(String),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SignalSdkOptions {
    pub sdk: String,
    pub sdk_version: Option<String>,
}

impl Default for SignalSdkOptions {
    fn default() -> Self {
        Self { sdk: "rust".to_string(), sdk_version: None }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SignalOptions {
    pub auto_subscribe: bool,
    pub adaptive_stream: bool,
    pub sdk_options: SignalSdkOptions,
}

impl Default for SignalOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
            sdk_options: SignalSdkOptions::default(),
        }
    }
}

pub enum SignalEvent {
    /// Received a message from the server
    Message(Box<proto::signal_response::Message>),

    /// Signal connection closed, SignalClient::restart() can be called to reconnect
    Close(Cow<'static, str>),
}

struct SignalInner {
    stream: AsyncRwLock<Option<SignalStream>>,
    token: Mutex<String>, // Token can be refreshed
    reconnecting: AtomicBool,
    queue: AsyncMutex<Vec<proto::signal_request::Message>>,
    url: String,
    options: SignalOptions,
    join_response: proto::JoinResponse,
    request_id: AtomicU32,
}

pub struct SignalClient {
    inner: Arc<SignalInner>,
    emitter: SignalEmitter,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Debug for SignalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalClient")
            .field("url", &self.url())
            .field("join_response", &self.join_response())
            .field("options", &self.options())
            .finish()
    }
}

impl SignalClient {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(Self, proto::JoinResponse, SignalEvents)> {
        let handle_success = |inner: Arc<SignalInner>, join_response, stream_events| {
            let (emitter, events) = mpsc::unbounded_channel();
            let signal_task =
                livekit_runtime::spawn(signal_task(inner.clone(), emitter.clone(), stream_events));

            (Self { inner, emitter, handle: Mutex::new(Some(signal_task)) }, join_response, events)
        };

        match SignalInner::connect(url, token, options.clone()).await {
            Ok((inner, join_response, stream_events)) => {
                return Ok(handle_success(inner, join_response, stream_events))
            }
            Err(err) => {
                // fallback to region urls
                if matches!(&err, SignalError::WsError(WsError::Http(e)) if e.status() != 403) {
                    log::error!("unexpected signal error: {}", err.to_string());
                }
                let urls = RegionUrlProvider::fetch_region_urls(url.into(), token.into()).await?;
                let mut last_err = err;

                for url in urls.iter() {
                    log::info!("fallback connection to: {}", url);
                    match SignalInner::connect(url, token, options.clone()).await {
                        Ok((inner, join_response, stream_events)) => {
                            return Ok(handle_success(inner, join_response, stream_events))
                        }
                        Err(err) => last_err = err,
                    }
                }

                Err(last_err)
            }
        }
    }

    /// Restart the connection to the server
    /// This will automatically flush the queue
    pub async fn restart(&self) -> SignalResult<proto::ReconnectResponse> {
        self.close().await;

        let (reconnect_response, stream_events) = self.inner.restart().await?;
        let signal_task = livekit_runtime::spawn(signal_task(
            self.inner.clone(),
            self.emitter.clone(),
            stream_events,
        ));

        *self.handle.lock() = Some(signal_task);
        Ok(reconnect_response)
    }

    /// Send a signal to the server (e.g. publish, subscribe, etc.)
    /// This will automatically queue the message if the connection fails
    /// The queue is flushed on the next restart
    pub async fn send(&self, signal: proto::signal_request::Message) {
        self.inner.send(signal).await
    }

    /// Close the connection to the server
    pub async fn close(&self) {
        self.inner.close(true).await;

        let handle = self.handle.lock().take();
        if let Some(signal_task) = handle {
            let _ = signal_task.await;
        }
    }

    /// Returns Initial JoinResponse
    pub fn join_response(&self) -> proto::JoinResponse {
        self.inner.join_response.clone()
    }

    /// Returns the initial options
    pub fn options(&self) -> SignalOptions {
        self.inner.options.clone()
    }

    /// Returns the initial URL
    pub fn url(&self) -> String {
        self.inner.url.clone()
    }

    /// Returns the last refreshed token (Or initial token if not refreshed yet)
    pub fn token(&self) -> String {
        self.inner.token.lock().clone()
    }

    /// Increment request_id for user-initiated requests and [`RequestResponse`][`proto::RequestResponse`]s
    pub fn next_request_id(&self) -> u32 {
        self.inner.next_request_id().clone()
    }
}

impl SignalInner {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
    ) -> SignalResult<(
        Arc<Self>,
        proto::JoinResponse,
        mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
    )> {
        let lk_url = get_livekit_url(url, &options)?;

        // Try to connect to the SignalClient
        let (stream, mut events) = match SignalStream::connect(lk_url.clone(), token).await {
            Ok(stream) => stream,
            Err(err) => {
                if let SignalError::TokenFormat = err {
                    return Err(err);
                }
                // Connection failed, try to retrieve more informations
                Self::validate(lk_url).await?;
                return Err(err);
            }
        };

        let join_response = get_join_response(&mut events).await?;

        // Successfully connected to the SignalClient
        let inner = Arc::new(SignalInner {
            stream: AsyncRwLock::new(Some(stream)),
            token: Mutex::new(token.to_owned()),
            reconnecting: AtomicBool::new(false),
            queue: Default::default(),
            options,
            url: url.to_string(),
            join_response: join_response.clone(),
            request_id: AtomicU32::new(1),
        });

        Ok((inner, join_response, events))
    }

    /// Validate the connection by calling rtc/validate
    async fn validate(mut ws_url: url::Url) -> SignalResult<()> {
        ws_url.set_scheme(if ws_url.scheme() == "wss" { "https" } else { "http" }).unwrap();

        if let Ok(mut segs) = ws_url.path_segments_mut() {
            segs.extend(&["rtc", "validate"]);
        }

        if let Ok(res) = http_client::get(ws_url.as_str()).await {
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
    pub async fn restart(
        self: &Arc<Self>,
    ) -> SignalResult<(
        proto::ReconnectResponse,
        mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
    )> {
        self.close(false).await;

        // Lock while we are reconnecting
        let mut stream = self.stream.write().await;

        self.reconnecting.store(true, Ordering::Release);
        scopeguard::defer!(self.reconnecting.store(false, Ordering::Release));

        let sid = &self.join_response.participant.as_ref().unwrap().sid;
        let token = self.token.lock().clone();

        let mut lk_url = get_livekit_url(&self.url, &self.options).unwrap();
        lk_url.query_pairs_mut().append_pair("reconnect", "1").append_pair("sid", sid);

        let (new_stream, mut events) = SignalStream::connect(lk_url, &token).await?;
        let reconnect_response = get_reconnect_response(&mut events).await?;
        *stream = Some(new_stream);

        drop(stream);
        self.flush_queue().await;
        Ok((reconnect_response, events))
    }

    /// Close the connection
    pub async fn close(&self, notify_close: bool) {
        if let Some(stream) = self.stream.write().await.take() {
            stream.close(notify_close).await;
        }
    }

    /// Send a signal to the server
    pub async fn send(&self, signal: proto::signal_request::Message) {
        if self.reconnecting.load(Ordering::Acquire) {
            self.queue_message(signal).await;
            return;
        }

        self.flush_queue().await; // The queue must be flusehd before sending any new signal

        if let Some(stream) = self.stream.read().await.as_ref() {
            if let Err(SignalError::SendError) = stream.send(signal.clone()).await {
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

        if let Some(stream) = self.stream.read().await.as_ref() {
            for signal in queue.drain(..) {
                // log::warn!("sending queued signal: {:?}", signal);

                if let Err(err) = stream.send(signal).await {
                    log::error!("failed to send queued signal: {}", err); // Lost message
                }
            }
        }
    }

    /// Increment request_id for user-initiated requests and [`RequestResponse`][`proto::RequestResponse`]s
    pub fn next_request_id(&self) -> u32 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }
}

/// Middleware task to receive SignalStream events and handle SignalClient specific logic
async fn signal_task(
    inner: Arc<SignalInner>,
    emitter: SignalEmitter, // Public emitter
    mut internal_events: mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
) {
    let mut ping_interval = interval(Duration::from_secs(inner.join_response.ping_interval as u64));
    let timeout_duration = Duration::from_secs(inner.join_response.ping_timeout as u64);
    let ping_timeout = sleep(timeout_duration);
    tokio::pin!(ping_timeout);

    let mut rtt = 0; // TODO(theomonnom): Should we expose SignalClient rtt?

    loop {
        tokio::select! {
            signal = internal_events.recv() => {
                if let Some(signal) = signal {
                    // Received a message from the server
                    match signal.as_ref() {
                        proto::signal_response::Message::RefreshToken(ref token) => {
                            // Refresh the token so the client can still reconnect if the initial join token expired
                            *inner.token.lock() = token.clone();
                        }
                        proto::signal_response::Message::PongResp(ref pong) => {
                            // Reset the ping_timeout if we received a pong
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as i64;

                            rtt = now - pong.last_ping_timestamp;
                        }
                        _ => {}
                    }

                    ping_timeout.as_mut().reset(Instant::now() + timeout_duration);

                    let _ = emitter.send(SignalEvent::Message(signal));
                } else {
                    let _ = emitter.send(SignalEvent::Close("stream closed".into()));
                    break; // Stream closed
                }
            }
            _ = ping_interval.tick() => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                let ping = proto::signal_request::Message::PingReq(proto::Ping{
                    timestamp: now,
                    rtt,
                });

                inner.send(ping).await;
            }
            _ = &mut ping_timeout => {
                let _ = emitter.send(SignalEvent::Close("ping timeout".into()));
                break;
            }
        }
    }

    inner.close(true).await; // Make sure to always close the ws connection when the loop is terminated
}

/// Check if the signal is queuable
/// Not every signal should be sent after signal reconnection
fn is_queuable(signal: &proto::signal_request::Message) -> bool {
    !matches!(
        signal,
        proto::signal_request::Message::SyncState(_)
            | proto::signal_request::Message::Trickle(_)
            | proto::signal_request::Message::Offer(_)
            | proto::signal_request::Message::Answer(_)
            | proto::signal_request::Message::Simulate(_)
            | proto::signal_request::Message::Leave(_)
    )
}

fn get_livekit_url(url: &str, options: &SignalOptions) -> SignalResult<url::Url> {
    let mut lk_url = url::Url::parse(url).map_err(|err| SignalError::UrlParse(err.to_string()))?;

    if !lk_url.has_host() {
        return Err(SignalError::UrlParse("missing host or scheme".into()));
    }

    // Automatically switch to websocket scheme when using user is providing http(s) scheme
    if lk_url.scheme() == "https" {
        lk_url.set_scheme("wss").unwrap();
    } else if lk_url.scheme() == "http" {
        lk_url.set_scheme("ws").unwrap();
    } else if lk_url.scheme() != "wss" && lk_url.scheme() != "ws" {
        return Err(SignalError::UrlParse(format!("unsupported scheme: {}", lk_url.scheme())));
    }

    if let Ok(mut segs) = lk_url.path_segments_mut() {
        segs.push("rtc");
    }

    lk_url
        .query_pairs_mut()
        .append_pair("sdk", options.sdk_options.sdk.as_str())
        .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str())
        .append_pair("auto_subscribe", if options.auto_subscribe { "1" } else { "0" })
        .append_pair("adaptive_stream", if options.adaptive_stream { "1" } else { "0" });

    if let Some(sdk_version) = &options.sdk_options.sdk_version {
        lk_url.query_pairs_mut().append_pair("version", sdk_version.as_str());
    }

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

            livekit_runtime::timeout(JOIN_RESPONSE_TIMEOUT, join).await.map_err(|_| {
                SignalError::Timeout(format!("failed to receive {}", std::any::type_name::<$ty>()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn livekit_url_test() {
        let io = SignalOptions::default();

        assert!(get_livekit_url("localhost:7880", &io).is_err());
        assert_eq!(get_livekit_url("https://localhost:7880", &io).unwrap().scheme(), "wss");
        assert_eq!(get_livekit_url("http://localhost:7880", &io).unwrap().scheme(), "ws");
        assert_eq!(get_livekit_url("wss://localhost:7880", &io).unwrap().scheme(), "wss");
        assert_eq!(get_livekit_url("ws://localhost:7880", &io).unwrap().scheme(), "ws");
        assert!(get_livekit_url("ftp://localhost:7880", &io).is_err());
    }
}
