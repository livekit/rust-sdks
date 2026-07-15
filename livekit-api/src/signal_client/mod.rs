// Copyright 2025 LiveKit, Inc.
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
    io::Write,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::URL_SAFE as BASE64_URL_SAFE, Engine};
use flate2::{write::GzEncoder, Compression};
use http::StatusCode;
use livekit_protocol as proto;
use livekit_runtime::{interval, sleep, Instant, JoinHandle};
use parking_lot::Mutex;
use prost::Message;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock as AsyncRwLock};

use crate::signal_client::signal_stream::SignalStream;

mod region_url_provider;
mod signal_stream;

pub use region_url_provider::RegionUrlProvider;

pub type SignalEmitter = mpsc::UnboundedSender<SignalEvent>;
pub type SignalEvents = mpsc::UnboundedReceiver<SignalEvent>;
pub type SignalResult<T> = Result<T, SignalError>;

pub const JOIN_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
pub const SIGNAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REGION_FETCH_TIMEOUT: Duration = Duration::from_secs(3);
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(3);
pub const PROTOCOL_VERSION: u32 = 17;

/// Capabilities the Rust SDK advertises to the SFU at connect time.
const CLIENT_CAPABILITIES: &[proto::client_info::Capability] =
    &[proto::client_info::Capability::CapPacketTrailer];

pub use livekit_common::{CLIENT_PROTOCOL_DATA_STREAM_RPC, CLIENT_PROTOCOL_DEFAULT};

/// The client protocol which is sent to other clients and indicates the set of apis that other
/// clients should assume this client supports.
const CLIENT_PROTOCOL_VERSION: i32 = CLIENT_PROTOCOL_DATA_STREAM_RPC;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SignalError {
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
    #[error("transport connection error: {0}")]
    Connection(String),
    #[error("transport closed")]
    Closed,
    /// No network transport is registered. On foreign/host builds the host must
    /// call `set_platform_transport` before connecting. This is a permanent
    /// configuration error, not a transient failure — callers must not retry.
    #[error("no network transport registered")]
    TransportNotConfigured,
    /// Failed to retrieve region information from LiveKit Cloud.
    ///
    /// This error occurs when the SDK cannot fetch the `/settings/regions` endpoint
    /// from LiveKit Cloud. The error message includes the full error chain to help
    /// diagnose the root cause.
    ///
    /// # Common Causes
    ///
    /// - **Missing CA certificates**: When deploying in containers using slim base images
    ///   (e.g., `node:*-slim`, `debian:*-slim`, Alpine), the system CA certificate store
    ///   may be empty. The error will include "invalid peer certificate: UnknownIssuer".
    ///
    ///   **Fix**: Install the `ca-certificates` package in your Dockerfile:
    ///   ```dockerfile
    ///   RUN apt-get update && apt-get install -y ca-certificates
    ///   ```
    ///
    ///   **Alternative**: Use the `rustls-tls-webpki-roots` feature instead of
    ///   `rustls-tls-native-roots` to bundle Mozilla's root certificates.
    ///
    /// - **Network connectivity issues**: The container cannot reach LiveKit Cloud endpoints.
    ///
    /// - **Invalid or expired access token**: The token used for authentication is not valid.
    #[error("failed to retrieve region info: {0}")]
    RegionError(String),
    #[error("server sent leave during reconnect: reason={reason:?}, action={action:?}")]
    LeaveRequest { reason: proto::DisconnectReason, action: proto::leave_request::Action },
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
    /// Enable single peer connection mode
    pub single_peer_connection: bool,
    /// Timeout for each individual signal connection attempt
    pub connect_timeout: Duration,
}

impl Default for SignalOptions {
    fn default() -> Self {
        Self {
            auto_subscribe: true,
            adaptive_stream: false,
            sdk_options: SignalSdkOptions::default(),
            single_peer_connection: false,
            connect_timeout: SIGNAL_CONNECT_TIMEOUT,
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
    /// Tracks whether single PC mode is active (v1 path succeeded)
    single_pc_mode_active: bool,
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
        publisher_offer: Option<proto::SessionDescription>,
    ) -> SignalResult<(Self, proto::JoinResponse, SignalEvents)> {
        let handle_success = |inner: Arc<SignalInner>, join_response, stream_events| {
            let (emitter, events) = mpsc::unbounded_channel();
            let signal_task =
                livekit_runtime::spawn(signal_task(inner.clone(), emitter.clone(), stream_events));

            (Self { inner, emitter, handle: Mutex::new(Some(signal_task)) }, join_response, events)
        };

        match SignalInner::connect(url, token, options.clone(), publisher_offer.clone()).await {
            Ok((inner, join_response, stream_events)) => {
                Ok(handle_success(inner, join_response, stream_events))
            }
            Err(err) => {
                // fallback to region urls
                if matches!(&err, SignalError::Client(code, _) if code.as_u16() != 403) {
                    log::error!("unexpected signal error: {}", err.to_string());
                }

                // Fetching region URLs is best-effort. `fetch_region_urls`
                // already returns an empty list for non-cloud (direct /
                // self-hosted) URLs, so those skip the fallback entirely. If the
                // fetch itself fails (e.g. the region endpoint is unreachable),
                // that must NOT be fatal: log a warning and fall back to the
                // original connection error rather than masking it with the
                // fetch error.
                let urls = match RegionUrlProvider::fetch_region_urls(url, token).await {
                    Ok(urls) => urls,
                    Err(region_err) => {
                        log::warn!(
                            "failed to fetch region urls: {region_err}; surfacing original connection error"
                        );
                        return Err(err);
                    }
                };

                // With no region URLs to try, this surfaces the original error.
                // Otherwise we keep the most recent region attempt error, so that
                // if every region fails the caller sees why the last region
                // connection failed.
                let mut last_err = err;
                for region_url in urls.iter() {
                    log::info!("fallback connection to: {}", region_url);
                    match SignalInner::connect(
                        region_url,
                        token,
                        options.clone(),
                        publisher_offer.clone(),
                    )
                    .await
                    {
                        Ok((inner, join_response, stream_events)) => {
                            return Ok(handle_success(inner, join_response, stream_events))
                        }
                        Err(region_conn_err) => {
                            // This region is unreachable; drop it from the cache
                            // so the next attempt doesn't hand it out again.
                            RegionUrlProvider::mark_failed(url, region_url);
                            last_err = region_conn_err;
                        }
                    }
                }

                Err(last_err)
            }
        }
    }

    /// Restart the connection to the server.
    ///
    /// Leaves the client in a "reconnecting" state with pass-through-only sends
    /// queueable signals (e.g. `AddTrack`, `Mute`, `UpdateSubscription`) accumulate
    /// in the queue. Caller MUST invoke [`Self::set_reconnected`] once the resume
    /// has fully recovered (PC connected, SyncState sent) to drain the queue and
    /// re-enable normal sends.
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

    /// Mark the signal as fully reconnected: drains the queue and clears the
    /// `reconnecting` flag so subsequent sends bypass the queue path.
    ///
    /// MUST be called by the engine after `wait_pc_reconnected` succeeds.
    /// Without this, the queued mutations (subscription updates, mutes, etc.)
    /// stay buffered indefinitely.
    pub async fn set_reconnected(&self) {
        self.inner.set_reconnected().await;
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

    /// Returns whether single peer connection mode is active.
    /// This is determined by whether the /rtc/v1 path was used successfully.
    pub fn is_single_pc_mode_active(&self) -> bool {
        self.inner.is_single_pc_mode_active()
    }

    /// Returns whether the underlying WebSocket is currently in place.
    ///
    /// The inner `signal_task` clears the stream slot when the WebSocket dies
    /// (ping timeout or remote close), so callers in the resume path can use
    /// this to detect "signal died again while we were waiting for the PC."
    /// Note: this does NOT inspect the `reconnecting` flag — during a normal
    /// resume the flag is true even after the new stream has been installed,
    /// and we want this check to return `true` in that case.
    pub async fn is_connected(&self) -> bool {
        self.inner.stream.read().await.is_some()
    }
}

impl SignalInner {
    pub async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
        publisher_offer: Option<proto::SessionDescription>,
    ) -> SignalResult<(
        Arc<Self>,
        proto::JoinResponse,
        mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
    )> {
        // Try v1 path first if single_peer_connection is enabled
        let use_v1_path = options.single_peer_connection;
        // For initial connection: reconnect=false, reconnect_reason=None, participant_sid=""
        let lk_url =
            get_livekit_url(url, &options, use_v1_path, false, None, "", publisher_offer.as_ref())?;
        // Try to connect to the SignalClient
        let (stream, mut events, single_pc_mode_active) =
            match SignalStream::connect(lk_url.clone(), token, options.connect_timeout).await {
                Ok((new_stream, stream_events)) => {
                    log::debug!(
                        "signal connection successful: path={}, single_pc_mode={}",
                        if use_v1_path { "v1" } else { "v0" },
                        use_v1_path
                    );
                    (new_stream, stream_events, use_v1_path)
                }
                Err(err) => {
                    log::warn!(
                        "signal connection failed on {} path: {:?}",
                        if use_v1_path { "v1" } else { "v0" },
                        err
                    );

                    if let SignalError::TokenFormat = err {
                        return Err(err);
                    }

                    // Only fallback to v0 if the v1 endpoint returned 404 (not found).
                    // Other errors (401, 403, 500, etc.) indicate real issues that shouldn't
                    // be masked by falling back to a different signaling mode.
                    let is_not_found =
                        matches!(&err, SignalError::Client(code, _) if code.as_u16() == 404);

                    if use_v1_path && is_not_found {
                        let lk_url_v0 =
                            get_livekit_url(url, &options, false, false, None, "", None)?;
                        log::warn!("v1 path not found (404), falling back to v0 path");
                        match SignalStream::connect(
                            lk_url_v0.clone(),
                            token,
                            options.connect_timeout,
                        )
                        .await
                        {
                            Ok((new_stream, stream_events)) => (new_stream, stream_events, false),
                            Err(err) => {
                                log::error!("v0 fallback also failed: {:?}", err);
                                if let SignalError::TokenFormat = err {
                                    return Err(err);
                                }
                                Self::validate(lk_url_v0, token).await?;
                                return Err(err);
                            }
                        }
                    } else {
                        // Connection failed, try to retrieve more information
                        Self::validate(lk_url, token).await?;
                        return Err(err);
                    }
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
            single_pc_mode_active,
        });

        Ok((inner, join_response, events))
    }

    /// Validate the connection by calling rtc/validate.
    ///
    /// This is called from `connect()` when the primary WebSocket upgrade fails
    /// with a non-404 status, to surface a clearer HTTP-level error than the WS
    /// upgrade error. The access token is sent as `Authorization: Bearer <token>`
    /// so the server can actually authenticate the request; without it, the
    /// server returns 401 "no permissions to access the room" regardless of
    /// what the original error was, masking the real cause (e.g. a 503 from a
    /// saturated node becomes a fabricated 401 to the caller). See
    /// https://github.com/livekit/rust-sdks/issues/1042.
    async fn validate(ws_url: url::Url, token: &str) -> SignalResult<()> {
        let validate_url = get_validate_url(ws_url);
        let transport = require_transport()?;
        let headers = bearer_headers(token);

        let validate_fut = async {
            // validate() is best-effort diagnostic enrichment: it turns a failed WS
            // upgrade into a clearer HTTP-level status error. A failure of the GET
            // *itself* (network/TLS error) carries no such information, so swallow
            // it and return Ok(()) — the caller then surfaces the original
            // connection error rather than this unrelated one. See issue #1042.
            let Ok(res) = transport.http_get(validate_url.to_string(), headers).await else {
                return Ok(());
            };
            // Fail closed on an out-of-range status (fall back to 502, matching the
            // region path) so a bogus status can't silently pass as 200/OK.
            let status =
                http::StatusCode::from_u16(res.status).unwrap_or(http::StatusCode::BAD_GATEWAY);
            if status.is_client_error() {
                return Err(SignalError::Client(
                    status,
                    String::from_utf8_lossy(&res.body).into_owned(),
                ));
            } else if status.is_server_error() {
                return Err(SignalError::Server(
                    status,
                    String::from_utf8_lossy(&res.body).into_owned(),
                ));
            }
            Ok(())
        };

        // A validate timeout is likewise non-fatal: fall through to Ok(()) so the
        // caller's original error is what surfaces, not a validate-timeout.
        livekit_runtime::timeout(VALIDATE_TIMEOUT, validate_fut).await.unwrap_or(Ok(()))
    }

    /// Returns whether single peer connection mode is active
    pub fn is_single_pc_mode_active(&self) -> bool {
        self.single_pc_mode_active
    }

    /// Restart is called when trying to resume the room (RtcSession resume).
    ///
    /// Leaves `reconnecting=true` on success — the engine is expected to call
    /// [`Self::set_reconnected`] once the full resume has succeeded. On failure
    /// resets `reconnecting=false` so subsequent retries can re-enter cleanly.
    /// The stream slot is held under a write lock for the entire close + new
    /// connect, so concurrent senders block on the read side until the new
    /// stream is in place.
    pub async fn restart(
        self: &Arc<Self>,
    ) -> SignalResult<(
        proto::ReconnectResponse,
        mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
    )> {
        // Set reconnecting BEFORE we touch the stream, so concurrent `send` calls
        // see the right state and route queueable messages to the queue (rather
        // than racing on a brief stream=None / reconnecting=false window).
        self.reconnecting.store(true, Ordering::Release);

        let mut stream_guard = self.stream.write().await;
        if let Some(old_stream) = stream_guard.take() {
            old_stream.close(false).await;
        }

        let sid = &self.join_response.participant.as_ref().unwrap().sid;
        let token = self.token.lock().clone();
        let lk_url = get_livekit_url(
            &self.url,
            &self.options,
            self.single_pc_mode_active,
            true,
            None,
            sid,
            None,
        )
        .unwrap();

        let result = async {
            let (new_stream, mut events) =
                SignalStream::connect(lk_url, &token, self.options.connect_timeout).await?;
            let reconnect_response = get_reconnect_response(&mut events).await?;
            SignalResult::Ok((new_stream, reconnect_response, events))
        }
        .await;

        match result {
            Ok((new_stream, reconnect_response, events)) => {
                *stream_guard = Some(new_stream);
                drop(stream_guard);
                // Note: NOT clearing `reconnecting` here. Caller must invoke
                // `set_reconnected()` after the resume has fully recovered.
                Ok((reconnect_response, events))
            }
            Err(err) => {
                // Connect / get_reconnect_response failed. Stream slot stays None.
                // Reset the flag so the next reconnect attempt can re-enter.
                drop(stream_guard);
                self.reconnecting.store(false, Ordering::Release);
                Err(err)
            }
        }
    }

    /// See [`SignalClient::set_reconnected`].
    pub async fn set_reconnected(&self) {
        // Order: clear the flag FIRST, then flush. This way any sends that race
        // with the flush see `reconnecting=false` and go through the normal path
        // (which itself flushes the queue), and we don't have queueable sends
        // sneaking back into the queue while we're trying to drain it.
        self.reconnecting.store(false, Ordering::Release);
        self.flush_queue().await;
    }

    /// Close the connection
    pub async fn close(&self, notify_close: bool) {
        if let Some(stream) = self.stream.write().await.take() {
            stream.close(notify_close).await;
        }
    }

    /// Send a signal to the server.
    ///
    /// During reconnect:
    /// - Pass-through signals (`Trickle`/`Offer`/`Answer`/`SyncState`/`Simulate`/`Leave`)
    ///   block on the stream lock and write through the new stream once it's in place.
    /// - Queueable signals are accumulated in the queue and drained by
    ///   [`Self::set_reconnected`] after the resume has fully recovered.
    pub async fn send(&self, signal: proto::signal_request::Message) {
        let pass_through = is_pass_through(&signal);
        let reconnecting = self.reconnecting.load(Ordering::Acquire);

        if reconnecting && !pass_through {
            // Queueable signal during reconnect — buffer for the post-resume flush.
            self.queue.lock().await.push(signal);
            return;
        }

        if !reconnecting {
            // Normal path: drain anything that was queued before the previous
            // reconnect, preserving the original send order.
            self.flush_queue().await;
        }

        // Pass-through during reconnect: the stream read lock is held by `restart`
        // until the new stream is installed, so this awaits and then writes via
        // the new stream. Same code path for the steady-state send — the lock is
        // free and we send immediately.
        if let Some(stream) = self.stream.read().await.as_ref() {
            if let Err(SignalError::SendError) = stream.send(signal.clone()).await {
                if !pass_through {
                    self.queue.lock().await.push(signal);
                } else {
                    log::warn!("dropping pass-through signal — send failed");
                }
            }
        } else if !pass_through {
            // Stream not in place AND signal is queueable — hold it.
            self.queue.lock().await.push(signal);
        } else {
            log::warn!("dropping pass-through signal — no stream available");
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
                // No pong within the configured window — the WS is dead even
                // if the OS hasn't told us yet. Tear down the stream and emit
                // Close; the engine layer reads that as a trigger to drive
                // a resume reconnect (see SignalEvent::Close docs).
                let _ = emitter.send(SignalEvent::Close("ping timeout".into()));
                break;
            }
        }
    }

    inner.close(true).await; // Make sure to always close the ws connection when the loop is terminated
}

/// Returns true for signals that must NOT be queued during a reconnect — they
/// drive signaling/negotiation itself (Trickle ICE candidates, the
/// publisher Offer, the subscriber Answer, the client SyncState that the SFU
/// uses to resync state, plus simulate/leave). Buffering these would deadlock
/// the resume. Mirrors `client-sdk-js` `passThroughQueueSignals`.
fn is_pass_through(signal: &proto::signal_request::Message) -> bool {
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

fn client_info_sdk_for_name(sdk: &str) -> proto::client_info::Sdk {
    match sdk {
        "js" => proto::client_info::Sdk::Js,
        "ios" | "swift" => proto::client_info::Sdk::Swift,
        "android" => proto::client_info::Sdk::Android,
        "flutter" => proto::client_info::Sdk::Flutter,
        "go" => proto::client_info::Sdk::Go,
        "unity" => proto::client_info::Sdk::Unity,
        "reactnative" => proto::client_info::Sdk::ReactNative,
        "rust" => proto::client_info::Sdk::Rust,
        "python" => proto::client_info::Sdk::Python,
        "cpp" => proto::client_info::Sdk::Cpp,
        "unityweb" => proto::client_info::Sdk::UnityWeb,
        "node" => proto::client_info::Sdk::Node,
        "esp32" => proto::client_info::Sdk::Esp32,
        _ => {
            log::warn!("unknown SDK name in signal options: {}", sdk);
            proto::client_info::Sdk::Unknown
        }
    }
}

/// Create the base64-encoded WrappedJoinRequest parameter required for v1 path
///
/// Parameters:
/// - options: SignalOptions containing auto_subscribe, adaptive_stream, etc.
/// - reconnect: true if this is a reconnection attempt
/// - participant_sid: the participant SID (only used during reconnection)
fn create_join_request_param(
    options: &SignalOptions,
    reconnect: bool,
    reconnect_reason: Option<i32>,
    participant_sid: &str,
    os: String,
    os_version: String,
    device_model: String,
    publisher_offer: Option<&proto::SessionDescription>,
) -> String {
    let connection_settings = proto::ConnectionSettings {
        auto_subscribe: options.auto_subscribe,
        adaptive_stream: options.adaptive_stream,
        ..Default::default()
    };

    let client_info = proto::ClientInfo {
        sdk: client_info_sdk_for_name(&options.sdk_options.sdk) as i32,
        version: options.sdk_options.sdk_version.clone().unwrap_or_default(),
        protocol: PROTOCOL_VERSION as i32,
        os,
        os_version,
        device_model,
        capabilities: CLIENT_CAPABILITIES.iter().map(|c| *c as i32).collect(),
        client_protocol: CLIENT_PROTOCOL_VERSION,
        ..Default::default()
    };

    let mut join_request = proto::JoinRequest {
        client_info: Some(client_info),
        connection_settings: Some(connection_settings),
        reconnect,
        publisher_offer: publisher_offer.cloned(),
        ..Default::default()
    };

    // Only set participant_sid if non-empty (for reconnects)
    if !participant_sid.is_empty() {
        join_request.participant_sid = participant_sid.to_string();
    }

    // Only set reconnect_reason if provided
    if let Some(reason) = reconnect_reason {
        join_request.reconnect_reason = reason;
    }

    // Serialize JoinRequest to bytes
    let join_request_bytes = join_request.encode_to_vec();

    // Always use gzip compression to reduce URL size on poor networks
    let (compressed_bytes, compression) = {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(&join_request_bytes).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                // Only use compressed version if it's actually smaller
                if compressed.len() < join_request_bytes.len() {
                    (compressed, proto::wrapped_join_request::Compression::Gzip as i32)
                } else {
                    (join_request_bytes, proto::wrapped_join_request::Compression::None as i32)
                }
            } else {
                (join_request_bytes, proto::wrapped_join_request::Compression::None as i32)
            }
        } else {
            (join_request_bytes, proto::wrapped_join_request::Compression::None as i32)
        }
    };

    let wrapped_join_request =
        proto::WrappedJoinRequest { join_request: compressed_bytes, compression };

    // Serialize WrappedJoinRequest to bytes and base64url encode
    // (URL-safe base64 avoids percent-encoding issues in query parameters)
    let wrapped_bytes = wrapped_join_request.encode_to_vec();
    BASE64_URL_SAFE.encode(&wrapped_bytes)
}

/// Build the LiveKit WebSocket URL for connection
///
/// Parameters:
/// - url: the base server URL
/// - options: SignalOptions
/// - use_v1_path: if true, use /rtc/v1 (single PC mode), otherwise /rtc (dual PC mode)
/// - reconnect: true if this is a reconnection attempt
/// - reconnect_reason: reason for reconnection (only used when reconnect=true)
/// - participant_sid: the participant SID (only used during reconnection)
fn get_livekit_url(
    url: &str,
    options: &SignalOptions,
    use_v1_path: bool,
    reconnect: bool,
    reconnect_reason: Option<i32>,
    participant_sid: &str,
    publisher_offer: Option<&proto::SessionDescription>,
) -> SignalResult<url::Url> {
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
        if use_v1_path {
            segs.push("v1");
        }
    }

    let os_info = os_info::get();
    let device_model = device_info::device_info().map(|info| info.model).unwrap_or_default();

    if use_v1_path {
        // For v1 path (single PC mode): only join_request param
        // All other info (sdk, protocol, auto_subscribe, etc.) is inside the JoinRequest protobuf
        let join_request_param = create_join_request_param(
            options,
            reconnect,
            reconnect_reason,
            participant_sid,
            os_info.os_type().to_string(),
            os_info.version().to_string(),
            device_model.to_string(),
            publisher_offer,
        );
        lk_url.query_pairs_mut().append_pair("join_request", &join_request_param);
    } else {
        // For v0 path (dual PC mode): use URL query parameters
        lk_url
            .query_pairs_mut()
            .append_pair("sdk", options.sdk_options.sdk.as_str())
            .append_pair("os", os_info.os_type().to_string().as_str())
            .append_pair("os_version", os_info.version().to_string().as_str())
            .append_pair("device_model", device_model.to_string().as_str())
            .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str())
            .append_pair("client_protocol", CLIENT_PROTOCOL_VERSION.to_string().as_str())
            .append_pair("auto_subscribe", if options.auto_subscribe { "1" } else { "0" })
            .append_pair("adaptive_stream", if options.adaptive_stream { "1" } else { "0" });

        if let Some(sdk_version) = &options.sdk_options.sdk_version {
            lk_url.query_pairs_mut().append_pair("version", sdk_version.as_str());
        }

        // parse client capabilities
        if !CLIENT_CAPABILITIES.is_empty() {
            let caps =
                CLIENT_CAPABILITIES.iter().map(|c| c.as_str_name()).collect::<Vec<_>>().join(",");
            lk_url.query_pairs_mut().append_pair("capabilities", &caps);
        }

        // For reconnects in v0 path, add reconnect and sid as separate query parameters
        if reconnect {
            lk_url
                .query_pairs_mut()
                .append_pair("reconnect", "1")
                .append_pair("sid", participant_sid);
        }
    }

    Ok(lk_url)
}

/// Build the `Authorization: Bearer <token>` header vec used by HTTP/WS callers.
pub(super) fn bearer_headers(token: &str) -> Vec<livekit_net::Header> {
    vec![livekit_net::Header { name: "Authorization".into(), value: format!("Bearer {token}") }]
}

/// Resolve the registered network transport, or a permanent
/// [`SignalError::TransportNotConfigured`] if none has been set. Centralises the
/// lookup so callers share one error rather than each inventing a string.
pub(super) fn require_transport() -> SignalResult<Arc<dyn livekit_net::PlatformTransport>> {
    livekit_net::transport().ok_or(SignalError::TransportNotConfigured)
}

/// Verify `token` can be encoded as an HTTP `Authorization` header value. A token
/// carrying characters illegal in a header value can never authenticate, so
/// callers surface a clear, non-retryable [`SignalError::TokenFormat`] up front
/// rather than a generic transport error deep in the connect path (which would
/// otherwise drive the pointless v1→v0 + full-reconnect fallback).
pub(super) fn check_token_format(token: &str) -> SignalResult<()> {
    http::HeaderValue::from_str(&format!("Bearer {token}"))
        .map(|_| ())
        .map_err(|_| SignalError::TokenFormat)
}

/// Map a [`livekit_net::TransportError`] to a [`SignalError`].
///
/// - `Timeout` → `SignalError::Timeout` (timed out at transport layer)
/// - `Http { status }` → `Client` for 4xx, `Server` for 5xx (empty body — caller
///   may have already read the body separately)
/// - `Connection` / `Other` → `SignalError::Connection` (network/TLS/transport failure)
/// - `Closed` → `SignalError::Closed` (peer/transport closed)
///
/// Every variant except `LeaveRequest` drives the engine's full-reconnect path.
pub(super) fn map_transport_err(e: livekit_net::TransportError) -> SignalError {
    use livekit_net::TransportError as TE;
    match e {
        TE::Timeout => SignalError::Timeout("transport timed out".into()),
        TE::Http { status } => {
            let code = http::StatusCode::from_u16(status)
                .unwrap_or(http::StatusCode::BAD_GATEWAY);
            if code.is_client_error() {
                SignalError::Client(code, String::new())
            } else {
                SignalError::Server(code, String::new())
            }
        }
        TE::Closed => SignalError::Closed,
        TE::Connection(m) => SignalError::Connection(m),
        TE::Other(m) => SignalError::Connection(m),
    }
}

/// Convert a WebSocket URL (with /rtc or /rtc/v1 path) to the validate endpoint URL
fn get_validate_url(mut ws_url: url::Url) -> url::Url {
    ws_url.set_scheme(if ws_url.scheme() == "wss" { "https" } else { "http" }).unwrap();
    // ws_url already has /rtc or /rtc/v1 from get_livekit_url, so only append /validate
    if let Ok(mut segs) = ws_url.path_segments_mut() {
        segs.push("validate");
    }
    ws_url
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

                Err(SignalError::Timeout("connection closed before message received".into()))
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

async fn get_reconnect_response(
    receiver: &mut mpsc::UnboundedReceiver<Box<proto::signal_response::Message>>,
) -> SignalResult<proto::ReconnectResponse> {
    let join = async {
        while let Some(event) = receiver.recv().await {
            match *event {
                proto::signal_response::Message::Reconnect(msg) => return Ok(msg),
                proto::signal_response::Message::Leave(leave) => {
                    return Err(SignalError::LeaveRequest {
                        reason: leave.reason(),
                        action: leave.action(),
                    });
                }
                _ => {}
            }
        }

        Err(SignalError::Timeout("connection closed before message received".into()))
    };

    livekit_runtime::timeout(JOIN_RESPONSE_TIMEOUT, join).await.map_err(|_| {
        SignalError::Timeout(format!(
            "failed to receive {}",
            std::any::type_name::<proto::ReconnectResponse>()
        ))
    })?
}
