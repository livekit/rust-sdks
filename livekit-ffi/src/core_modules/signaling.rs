// Copyright 2026 LiveKit, Inc.
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

//! The signalling client from [`livekit_signaling`], for foreign callers.
//!
//! The host supplies the transport: register a WebSocket client with
//! [`livekit_net::set_ws_client`] before calling [`connect_signal_session`], or it fails with
//! [`SignalError::TransportNotConfigured`].
//!
//! Protobuf crosses this boundary as encoded bytes, as it does for data tracks: [`send`] takes an
//! encoded `livekit.SignalRequest`, and [`SignalEvent::Signal`] carries an encoded
//! `livekit.SignalResponse`.
//!
//! [`send`]: SignalSession::send

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use livekit_protocol as proto;
use livekit_signaling as signaling;
use prost::Message;
use tokio_util::sync::CancellationToken;

// MARK: - Options

/// Identifies the calling SDK to the server. FFI wrapper around
/// [`signaling::SignalSdkOptions`].
#[derive(uniffi::Record)]
pub struct SignalSdkOptions {
    /// SDK name, e.g. `"swift"`. Unrecognized names are reported as unknown.
    pub sdk: String,
    #[uniffi(default = None)]
    pub sdk_version: Option<String>,
    /// Comma separated list of SDKs layered on top of this one, with versions.
    #[uniffi(default = None)]
    pub other_sdks: Option<String>,
}

impl From<SignalSdkOptions> for signaling::SignalSdkOptions {
    fn from(options: SignalSdkOptions) -> Self {
        // `signaling::SignalSdkOptions` is `#[non_exhaustive]`, so it cannot be built from a
        // struct literal.
        let mut out = signaling::SignalSdkOptions::default();
        out.sdk = options.sdk;
        out.sdk_version = options.sdk_version;
        out.other_sdks = options.other_sdks;
        out
    }
}

/// Options for [`connect_signal_session`]. FFI wrapper around [`signaling::SignalOptions`].
#[derive(uniffi::Record)]
pub struct SignalOptions {
    #[uniffi(default = true)]
    pub auto_subscribe: bool,
    #[uniffi(default = false)]
    pub adaptive_stream: bool,
    pub sdk_options: SignalSdkOptions,
    /// Enable single peer connection mode.
    #[uniffi(default = false)]
    pub single_peer_connection: bool,
    /// Timeout for each individual connection attempt. `None` uses the crate default.
    #[uniffi(default = None)]
    pub connect_timeout: Option<Duration>,
    /// Advertise only legacy data stream support.
    #[uniffi(default = false)]
    pub use_legacy_data_streams: bool,
}

impl From<SignalOptions> for signaling::SignalOptions {
    fn from(options: SignalOptions) -> Self {
        // `signaling::SignalOptions` is `#[non_exhaustive]`, so it cannot be built from a struct
        // literal.
        let mut out = signaling::SignalOptions::default();
        out.auto_subscribe = options.auto_subscribe;
        out.adaptive_stream = options.adaptive_stream;
        out.sdk_options = options.sdk_options.into();
        out.single_peer_connection = options.single_peer_connection;
        out.use_legacy_data_streams = options.use_legacy_data_streams;
        if let Some(connect_timeout) = options.connect_timeout {
            out.connect_timeout = connect_timeout;
        }
        out
    }
}

// MARK: - Enums

/// Why the client is resuming its session, reported to the server. Mirrors
/// [`proto::ReconnectReason`].
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconnectReason {
    Unknown,
    SignalDisconnected,
    PublisherFailed,
    SubscriberFailed,
    SwitchCandidate,
}

impl From<ReconnectReason> for proto::ReconnectReason {
    fn from(reason: ReconnectReason) -> Self {
        match reason {
            ReconnectReason::Unknown => Self::RrUnknown,
            ReconnectReason::SignalDisconnected => Self::RrSignalDisconnected,
            ReconnectReason::PublisherFailed => Self::RrPublisherFailed,
            ReconnectReason::SubscriberFailed => Self::RrSubscriberFailed,
            ReconnectReason::SwitchCandidate => Self::RrSwitchCandidate,
        }
    }
}

/// An item from a session's event stream; see [`SignalEventQueue::next_event`].
#[derive(uniffi::Enum, Debug)]
pub enum SignalEvent {
    /// An encoded `livekit.SignalResponse` from the server.
    Signal { payload: Bytes },
    /// The signalling link went away. [`SignalSession::restart`] can resume the session.
    Closed { reason: String },
}

impl From<signaling::SignalEvent> for SignalEvent {
    fn from(event: signaling::SignalEvent) -> Self {
        match event {
            // The Rust event carries the inner oneof; foreign callers decode a whole
            // `SignalResponse`, which is also what this crate's data-track wrappers accept back.
            signaling::SignalEvent::Message(message) => Self::Signal {
                payload: Bytes::from(
                    proto::SignalResponse { message: Some(*message) }.encode_to_vec(),
                ),
            },
            signaling::SignalEvent::Close(reason) => Self::Closed { reason: reason.into_owned() },
        }
    }
}

// MARK: - Errors

/// A signalling operation failed.
///
/// Only the cases a caller can act on differently are broken out; everything else arrives as
/// [`Other`](Self::Other) carrying the underlying message.
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum SignalError {
    #[error("client error: {status} - {body}")]
    Client { status: u16, body: String },

    #[error("server error: {status} - {body}")]
    Server { status: u16, body: String },

    /// HTTP status from a failed WebSocket upgrade. Retryable, unlike
    /// [`Client`](Self::Client) with a 401 or 403.
    #[error("handshake error: {status}")]
    Handshake { status: u16 },

    /// No network transport is registered. Permanent: register one with
    /// [`livekit_net::set_ws_client`] rather than retrying.
    #[error("no network transport registered")]
    TransportNotConfigured,

    /// The server asked the client to leave during a resume. `disconnect_reason` and `action`
    /// are the numeric values of `livekit.DisconnectReason` and `livekit.LeaveRequest.Action`.
    #[error("server sent leave during reconnect: reason={disconnect_reason}, action={action}")]
    LeaveRequest { disconnect_reason: i32, action: i32 },

    /// Everything else: a bad URL or token, a decode failure, a timeout, a dead transport, or a
    /// panic in the task running the operation.
    #[error("{reason}")]
    Other { reason: String },
}

impl From<signaling::SignalError> for SignalError {
    fn from(error: signaling::SignalError) -> Self {
        use signaling::SignalError as E;
        match error {
            E::Client(status, body) => Self::Client { status: status.as_u16(), body },
            E::Server(status, body) => Self::Server { status: status.as_u16(), body },
            E::Handshake { status } => Self::Handshake { status: status.as_u16() },
            E::TransportNotConfigured => Self::TransportNotConfigured,
            E::LeaveRequest { reason, action } => {
                Self::LeaveRequest { disconnect_reason: reason as i32, action: action as i32 }
            }
            // Catches the variants not broken out above, and is required besides:
            // `signaling::SignalError` is `#[non_exhaustive]`.
            ref other => Self::Other { reason: other.to_string() },
        }
    }
}

/// The error a resume returns once the session has been closed.
fn closed_resume() -> SignalError {
    SignalError::Other { reason: "cannot resume a closed session".to_string() }
}

/// A request handed to [`SignalSession::send`] could not be sent.
#[derive(uniffi::Error, thiserror::Error, Debug)]
pub enum SignalRequestError {
    #[error("failed to decode the signal request: {reason}")]
    Decode { reason: String },

    #[error("the signal request carried no message")]
    EmptyRequest,

    /// The session is closed, so the request would have been discarded.
    #[error("the session is closed")]
    Closed,
}

// MARK: - Event queue

/// The server-sent half of a session, pulled one event at a time.
///
/// Pull rather than push: a uniffi callback invoked from a tokio worker aborts the Dart VM
/// outright, and the underlying receiver already exists, so no delegate is needed.
///
/// The queue behind it is unbounded, so a host that wants no events must release it rather
/// than leave it undrained: releasing drops the receiver, after which the sender fails cheaply.
#[derive(uniffi::Object)]
pub struct SignalEventQueue {
    events: tokio::sync::Mutex<signaling::SignalEvents>,
    shutdown: CancellationToken,
}

#[uniffi::export(async_runtime = "tokio")]
impl SignalEventQueue {
    /// Awaits the next event from the server.
    ///
    /// Returns `None` once the session has shut down, which ends the caller's drain loop.
    /// Call [`SignalSession::close`] to end it: releasing the session handle also works, but
    /// only once nothing else holds it. Release this queue last either way, because a caller
    /// blocked here is holding a pointer to it, so freeing it first is a use-after-free.
    pub async fn next_event(&self) -> Option<SignalEvent> {
        let mut events = self.events.lock().await;
        let event = tokio::select! {
            // Biased: a close makes both branches ready, and the shutdown is the answer the
            // caller asked for. An unbiased pick could hand back the `Closed` event instead,
            // which reads as resumable.
            biased;
            _ = self.shutdown.cancelled() => return None,
            received = events.recv() => received?,
        };
        Some(event.into())
    }
}

// MARK: - Session

/// A connected signalling session.
#[derive(uniffi::Object)]
pub struct SignalSession {
    client: Arc<signaling::SignalClient>,
    shutdown: CancellationToken,
}

#[uniffi::export(async_runtime = "tokio")]
impl SignalSession {
    /// Sends an encoded `livekit.SignalRequest` to the server.
    ///
    /// Session-scoped requests are queued while a resume is in flight and flushed by
    /// [`set_reconnected`](Self::set_reconnected). Fails with
    /// [`SignalRequestError::Closed`] once the session has been closed, because a request
    /// handed over after that is dropped or queued behind a flush that will never run.
    pub async fn send(&self, request: &[u8]) -> Result<(), SignalRequestError> {
        if self.shutdown.is_cancelled() {
            return Err(SignalRequestError::Closed);
        }
        let request = proto::SignalRequest::decode(request)
            .map_err(|err| SignalRequestError::Decode { reason: err.to_string() })?;
        let Some(message) = request.message else {
            return Err(SignalRequestError::EmptyRequest);
        };
        self.client.send(message).await;
        Ok(())
    }

    /// Resumes the session on a new transport, returning an encoded
    /// `livekit.ReconnectResponse`.
    ///
    /// Leaves the session reconnecting: call [`set_reconnected`](Self::set_reconnected) once the
    /// media path is back, or queued requests stay buffered. Fails once the session has been
    /// closed.
    pub async fn restart(&self, reason: ReconnectReason) -> Result<Bytes, SignalError> {
        if self.shutdown.is_cancelled() {
            return Err(closed_resume());
        }
        let client = self.client.clone();
        let shutdown = self.shutdown.clone();
        // As in `connect_signal_session`: `restart` spawns a replacement signal task.
        let response = crate::runtime::runtime()
            .spawn(async move {
                let response = client.restart(reason.into()).await;
                // A close that lands mid-resume cannot stop it: `SignalClient::close` returns
                // early once the state is `Disconnecting`, but the state machine still accepts
                // the resume afterwards. Closing here is what keeps the new signal task owned,
                // and the resume is reported as refused however late the close arrived.
                if shutdown.is_cancelled() {
                    client.close().await;
                    return Err(closed_resume());
                }
                response.map_err(SignalError::from)
            })
            .await
            .map_err(|err| SignalError::Other {
                reason: format!("restart task failed: {err}"),
            })??;
        Ok(Bytes::from(response.encode_to_vec()))
    }

    /// Marks the resume complete, which flushes requests held during it.
    pub async fn set_reconnected(&self) {
        self.client.set_reconnected().await;
    }

    /// Closes the session and ends its event stream.
    ///
    /// Final: a closed session cannot be resumed with [`restart`](Self::restart).
    pub async fn close(&self) {
        self.shutdown.cancel();
        self.client.close().await;
    }

    /// Whether the underlying WebSocket is currently in place.
    pub async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }

    /// The last refreshed token, or the initial one if the server has not refreshed it.
    pub fn token(&self) -> String {
        self.client.token()
    }

    /// Returns the next request id, for user-initiated requests and their responses.
    pub fn next_request_id(&self) -> u32 {
        self.client.next_request_id()
    }

    /// Whether single peer connection mode is active.
    pub fn is_single_pc_mode_active(&self) -> bool {
        self.client.is_single_pc_mode_active()
    }
}

impl Drop for SignalSession {
    fn drop(&mut self) {
        // Cancel first, so a caller already parked in `next_event` is released.
        self.shutdown.cancel();
        // `drop` cannot await, so the close runs as a task holding the last reference to the
        // client. It is bounded: `close` only waits on the signal task, which ends with the
        // transport. Releasing the client afterwards drops the emitter and ends the event stream.
        let client = self.client.clone();
        crate::runtime::runtime().spawn(async move { client.close().await });
    }
}

/// A session and the two handles that drive it, returned by [`connect_signal_session`].
#[derive(uniffi::Record)]
pub struct SignalSessionConnected {
    pub session: Arc<SignalSession>,
    /// Encoded `livekit.JoinResponse`.
    pub join_response: Bytes,
    pub events: Arc<SignalEventQueue>,
}

type Connected = (signaling::SignalClient, proto::JoinResponse, signaling::SignalEvents);

/// Holds the in-flight connect task, and closes whatever it produces if the caller goes away.
///
/// Dropping a [`tokio::task::JoinHandle`] detaches the task rather than cancelling it, so a
/// cancelled `connect` still opens a session that nobody holds. `SignalClient` has no `Drop`,
/// so without this the signal task keeps the WebSocket alive and the server keeps a participant
/// in the room.
struct CloseOnCancel(Option<tokio::task::JoinHandle<signaling::SignalResult<Connected>>>);

impl CloseOnCancel {
    /// Awaits the connect task, disarming the guard once it resolves.
    async fn join(&mut self) -> Result<signaling::SignalResult<Connected>, tokio::task::JoinError> {
        let connected = self.0.as_mut().expect("armed until joined").await;
        self.0 = None;
        connected
    }
}

impl Drop for CloseOnCancel {
    fn drop(&mut self) {
        let Some(handle) = self.0.take() else { return };
        crate::runtime::runtime().spawn(async move {
            if let Ok(Ok((client, ..))) = handle.await {
                client.close().await;
            }
        });
    }
}

/// Connects a signalling session to `url`.
///
/// `publisher_offer` is an encoded `livekit.SessionDescription`, used by single peer connection
/// mode. The host must have registered a transport with [`livekit_net::set_ws_client`].
#[uniffi::export(async_runtime = "tokio")]
pub async fn connect_signal_session(
    url: String,
    token: String,
    options: SignalOptions,
    publisher_offer: Option<Vec<u8>>,
) -> Result<SignalSessionConnected, SignalError> {
    let publisher_offer = publisher_offer
        .map(|offer| proto::SessionDescription::decode(offer.as_slice()))
        .transpose()
        .map_err(|err| SignalError::Other { reason: err.to_string() })?;
    let options = options.into();

    // `SignalClient::connect` spawns the long-lived signal task, which must land on this crate's
    // runtime rather than whatever ambient one uniffi supplies. Spawning needs a `'static`
    // future, which is why `url` and `token` are owned rather than `&str`.
    let mut connect = CloseOnCancel(Some(crate::runtime::runtime().spawn(async move {
        signaling::SignalClient::connect(&url, &token, options, publisher_offer).await
    })));
    let (client, join_response, events) = connect
        .join()
        .await
        .map_err(|err| SignalError::Other { reason: format!("connect task failed: {err}") })??;

    let shutdown = CancellationToken::new();
    Ok(SignalSessionConnected {
        session: Arc::new(SignalSession { client: Arc::new(client), shutdown: shutdown.clone() }),
        join_response: Bytes::from(join_response.encode_to_vec()),
        events: Arc::new(SignalEventQueue { events: tokio::sync::Mutex::new(events), shutdown }),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, Once};

    use livekit_net::{Header, TransportError, WsClient, WsConnectResult, WsConnection};
    use tokio::sync::mpsc;

    use super::*;

    const URL: &str = "ws://localhost:7880";
    const TOKEN: &str = "test-token";

    /// The mock is registered process-wide and hands its connection back through a global slot,
    /// so the tests that use it run one at a time.
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static LATEST_CONN: Mutex<Option<Arc<MockConn>>> = Mutex::new(None);
    static INSTALL: Once = Once::new();
    /// Leaves a connect or resume hanging on its response, so a test can cancel it mid-flight.
    static WITHHOLD_RESPONSE: AtomicBool = AtomicBool::new(false);
    /// Makes the transport refuse the WebSocket upgrade.
    static FAIL_CONNECT: AtomicBool = AtomicBool::new(false);
    /// Stalls the transport's close, which holds a resume inside its own teardown step.
    static STALL_CLOSE: AtomicBool = AtomicBool::new(false);

    /// Clears the mock's state when a test ends, panic or not. A flag left set makes every
    /// later test hang until `JOIN_RESPONSE_TIMEOUT` and fail for an unrelated reason, which
    /// hides whichever assertion actually broke; a connection left in the slot is worse, since
    /// the next test can take it and assert against an already-released transport.
    struct ResetFlags;

    impl Drop for ResetFlags {
        fn drop(&mut self) {
            WITHHOLD_RESPONSE.store(false, Ordering::SeqCst);
            FAIL_CONNECT.store(false, Ordering::SeqCst);
            STALL_CLOSE.store(false, Ordering::SeqCst);
            *LATEST_CONN.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
    }

    /// The `JoinResponse` the mock answers every connect with.
    ///
    /// `ping_interval` and `ping_timeout` must be non-zero: the signal task builds a
    /// [`tokio::time::interval`] from the former, which panics on a zero duration. `participant`
    /// must be set because [`signaling::SignalClient::restart`] unwraps it.
    fn join_response() -> proto::JoinResponse {
        proto::JoinResponse {
            ping_interval: 30,
            ping_timeout: 30,
            participant: Some(proto::ParticipantInfo {
                sid: "PA_test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// The `ReconnectResponse` the mock answers every resume with.
    fn reconnect_response() -> proto::ReconnectResponse {
        proto::ReconnectResponse { last_message_seq: 7, ..Default::default() }
    }

    fn encode(message: proto::signal_response::Message) -> Vec<u8> {
        proto::SignalResponse { message: Some(message) }.encode_to_vec()
    }

    struct MockConn {
        inbound_tx: mpsc::UnboundedSender<Vec<u8>>,
        inbound: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
        sent_tx: mpsc::UnboundedSender<Vec<u8>>,
        sent: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    }

    impl MockConn {
        fn new() -> Self {
            let (inbound_tx, inbound) = mpsc::unbounded_channel();
            let (sent_tx, sent) = mpsc::unbounded_channel();
            Self {
                inbound_tx,
                inbound: tokio::sync::Mutex::new(inbound),
                sent_tx,
                sent: tokio::sync::Mutex::new(sent),
            }
        }

        /// Queue a frame for the client to read.
        fn push(&self, frame: Vec<u8>) {
            self.inbound_tx.send(frame).expect("the mock owns the receiver");
        }

        /// Take the oldest frame the client wrote, waiting briefly for it to arrive.
        ///
        /// Pings are skipped: `signal_task`'s interval fires its first tick immediately, so a
        /// `PingReq` reaches the wire before anything a test sends.
        async fn next_sent(&self) -> proto::SignalRequest {
            let mut sent = self.sent.lock().await;
            loop {
                let frame = tokio::time::timeout(Duration::from_secs(1), sent.recv())
                    .await
                    .expect("the client sent nothing")
                    .expect("the mock owns the sender");
                let request = proto::SignalRequest::decode(frame.as_slice())
                    .expect("the client writes encoded SignalRequests");
                if !matches!(request.message, Some(proto::signal_request::Message::PingReq(_))) {
                    return request;
                }
            }
        }
    }

    #[async_trait::async_trait]
    impl WsConnection for MockConn {
        async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError> {
            self.sent_tx.send(frame).expect("the mock owns the receiver");
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
            // Never `Ok(None)`: that is a peer close, which would tear the session down between
            // the frames a test pushes. The channel's sender lives here, so this cannot end.
            match self.inbound.lock().await.recv().await {
                Some(frame) => Ok(Some(frame)),
                None => std::future::pending().await,
            }
        }

        async fn close(&self) {
            if STALL_CLOSE.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    }

    struct MockClient;

    #[async_trait::async_trait]
    impl WsClient for MockClient {
        async fn connect(
            &self,
            url: String,
            _headers: Vec<Header>,
            _timeout_ms: u64,
        ) -> Result<WsConnectResult, TransportError> {
            if FAIL_CONNECT.load(Ordering::SeqCst) {
                return Err(TransportError::Http { status: 503 });
            }
            let conn = Arc::new(MockConn::new());
            if !WITHHOLD_RESPONSE.load(Ordering::SeqCst) {
                // A resume carries `reconnect=1` and waits for a `ReconnectResponse`; the
                // initial connect waits for a `JoinResponse`.
                conn.push(if url.contains("reconnect=1") {
                    encode(proto::signal_response::Message::Reconnect(reconnect_response()))
                } else {
                    encode(proto::signal_response::Message::Join(join_response()))
                });
            }
            *LATEST_CONN.lock().unwrap() = Some(conn.clone());
            Ok(WsConnectResult { connection: conn })
        }
    }

    // `SignalInner::connect` calls `validate()` on a failed upgrade, which needs an HTTP client
    // or the real error is replaced by `TransportNotConfigured`.
    #[async_trait::async_trait]
    impl livekit_net::HttpClient for MockClient {
        async fn request(
            &self,
            _method: livekit_net::HttpMethod,
            _url: String,
            _headers: Vec<Header>,
            _body: Option<Vec<u8>>,
        ) -> Result<livekit_net::HttpResponse, TransportError> {
            Ok(livekit_net::HttpResponse { status: 200, headers: vec![], body: vec![] })
        }
    }

    /// Register the mock transport, once per process: `set_ws_client` writes a `OnceLock` and
    /// the first registration wins.
    fn install_mock_transport() {
        INSTALL.call_once(|| {
            livekit_net::set_ws_client(Arc::new(MockClient));
            livekit_net::set_http_client(Arc::new(MockClient));
        });
    }

    /// Take the connection the last connect opened, so the caller holds the only reference
    /// outside the client and can watch its strong count fall back to one on teardown.
    fn take_conn() -> Arc<MockConn> {
        LATEST_CONN.lock().unwrap().take().expect("a connection was opened")
    }

    /// Wait for `condition` to hold, failing with `what` if it has not within five seconds.
    ///
    /// Teardown here is asynchronous — a `Drop` spawns the close — so the state a test asserts
    /// on arrives shortly after the call that causes it, not during.
    async fn eventually(mut condition: impl FnMut() -> bool, what: &str) {
        let polling = async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), polling).await.expect(what);
    }

    /// Wait for the transport to release `conn`, which is how teardown is observable here.
    async fn assert_released(conn: Arc<MockConn>) {
        eventually(|| Arc::strong_count(&conn) == 1, "the transport outlived its session").await;
    }

    fn options() -> SignalOptions {
        SignalOptions {
            auto_subscribe: true,
            adaptive_stream: false,
            sdk_options: SignalSdkOptions {
                sdk: "rust".to_string(),
                sdk_version: None,
                other_sdks: None,
            },
            single_peer_connection: false,
            connect_timeout: None,
            use_legacy_data_streams: false,
        }
    }

    /// Run `test` on the global runtime, holding the lock the shared mock transport needs.
    fn with_mock_transport<F>(test: F)
    where
        F: std::future::Future<Output = ()>,
    {
        install_mock_transport();
        let _lock = TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _flags = ResetFlags;
        crate::runtime::runtime().block_on(test);
    }

    async fn connect() -> SignalSessionConnected {
        connect_signal_session(URL.to_string(), TOKEN.to_string(), options(), None)
            .await
            .expect("the mock transport answers with a JoinResponse")
    }

    #[test]
    fn connect_returns_the_join_response_and_streams_events() {
        with_mock_transport(async {
            let connected = connect().await;
            assert_eq!(
                proto::JoinResponse::decode(connected.join_response.as_ref()).unwrap(),
                join_response()
            );
            assert_eq!(connected.session.token(), TOKEN);

            let pong = proto::signal_response::Message::PongResp(proto::Pong {
                last_ping_timestamp: 7,
                timestamp: 9,
            });
            take_conn().push(encode(pong.clone()));

            let event = tokio::time::timeout(Duration::from_secs(5), connected.events.next_event())
                .await
                .expect("an event should arrive")
                .expect("the stream is still open");
            let SignalEvent::Signal { payload } = event else {
                panic!("expected a server message, got {event:?}");
            };
            assert_eq!(
                proto::SignalResponse::decode(payload.as_ref()).unwrap().message,
                Some(pong)
            );
        });
    }

    #[test]
    fn send_encodes_requests_and_rejects_unreadable_ones() {
        with_mock_transport(async {
            let connected = connect().await;
            let conn = take_conn();

            let message = proto::signal_request::Message::Leave(proto::LeaveRequest::default());
            let request = proto::SignalRequest { message: Some(message.clone()) }.encode_to_vec();
            connected.session.send(&request).await.expect("a well-formed request is accepted");
            assert_eq!(conn.next_sent().await.message, Some(message));

            let empty = proto::SignalRequest { message: None }.encode_to_vec();
            assert!(matches!(
                connected.session.send(&empty).await,
                Err(SignalRequestError::EmptyRequest)
            ));

            // A field header for a field number that does not exist, with a truncated body.
            assert!(matches!(
                connected.session.send(&[0xff, 0xff, 0xff]).await,
                Err(SignalRequestError::Decode { .. })
            ));
        });
    }

    /// The initialize → use → drop → shutdown workload. Repeated, so that a client retained by a
    /// stray reference shows up as a failure rather than passing by luck.
    #[test]
    fn dropping_the_session_shuts_it_down() {
        with_mock_transport(async {
            for _ in 0..3 {
                let connected = connect().await;
                let events = connected.events.clone();
                let client = Arc::downgrade(&connected.session.client);
                let conn = take_conn();

                conn.push(encode(
                    proto::signal_response::Message::PongResp(proto::Pong::default()),
                ));
                tokio::time::timeout(Duration::from_secs(5), events.next_event())
                    .await
                    .expect("an event should arrive")
                    .expect("the stream is still open");

                drop(connected);

                assert!(
                    tokio::time::timeout(Duration::from_secs(5), events.next_event())
                        .await
                        .expect("the event stream should end")
                        .is_none(),
                    "the event stream outlived the session"
                );

                eventually(|| client.upgrade().is_none(), "the signal client outlived its session")
                    .await;
                // The client being gone does not prove the socket is: the read and write tasks
                // hold it too.
                assert_released(conn).await;
            }
        });
    }

    /// A caller that cancels mid-connect must not leave a session running. Dropping the task
    /// handle only detaches it, so the connect still completes and something has to close it.
    #[test]
    fn cancelling_connect_closes_the_session() {
        with_mock_transport(async {
            WITHHOLD_RESPONSE.store(true, Ordering::SeqCst);
            let mut connect = Box::pin(connect_signal_session(
                URL.to_string(),
                TOKEN.to_string(),
                options(),
                None,
            ));

            // Drive it until the transport is open and it is waiting on the JoinResponse.
            let conn = loop {
                assert!(
                    tokio::time::timeout(Duration::from_millis(50), &mut connect).await.is_err(),
                    "connect should still be waiting for the JoinResponse"
                );
                if let Some(conn) = LATEST_CONN.lock().unwrap().take() {
                    break conn;
                }
            };

            drop(connect);
            WITHHOLD_RESPONSE.store(false, Ordering::SeqCst);
            conn.push(encode(proto::signal_response::Message::Join(join_response())));

            assert_released(conn).await;
        });
    }

    #[test]
    fn restart_resumes_the_session_and_is_refused_after_close() {
        with_mock_transport(async {
            let connected = connect().await;
            take_conn();

            let response = connected
                .session
                .restart(ReconnectReason::SignalDisconnected)
                .await
                .expect("the mock answers a resume with a ReconnectResponse");
            assert_eq!(
                proto::ReconnectResponse::decode(response.as_ref()).unwrap(),
                reconnect_response()
            );

            connected.session.close().await;
            take_conn();

            let Err(error) = connected.session.restart(ReconnectReason::SignalDisconnected).await
            else {
                panic!("a closed session cannot be resumed");
            };
            assert!(
                matches!(&error, SignalError::Other { reason } if reason.contains("closed")),
                "expected a closed-session error, got {error:?}"
            );
            // Refused before it reached the transport, not after: a resume that got as far as
            // dialling would have left a connection in the slot.
            assert!(
                LATEST_CONN.lock().unwrap().is_none(),
                "a refused resume should not open a transport"
            );

            // `SignalClient::send` returns `()`, so a request handed over after the close is
            // dropped or queued behind a flush that never runs. Reporting that as success would
            // tell a host its request is on the wire.
            let leave = proto::SignalRequest {
                message: Some(
                    proto::signal_request::Message::Leave(proto::LeaveRequest::default()),
                ),
            }
            .encode_to_vec();
            assert!(matches!(
                connected.session.send(&leave).await,
                Err(SignalRequestError::Closed)
            ));
        });
    }

    /// The resume counterpart of `cancelling_connect_closes_the_session`.
    ///
    /// `SignalClient::restart` tears the old transport down before it resumes, and the state is
    /// `Disconnecting` for the duration. A close arriving in that window is refused by the
    /// transition table and returns having done nothing, while the resume carries on and
    /// installs a replacement nobody holds. Stalling the transport's own close widens that
    /// window enough to land a drop inside it.
    #[test]
    fn dropping_mid_restart_closes_the_resumed_session() {
        with_mock_transport(async {
            let connected = connect().await;
            let first = take_conn();

            STALL_CLOSE.store(true, Ordering::SeqCst);
            let session = connected.session.clone();
            let mut restart =
                Box::pin(async move { session.restart(ReconnectReason::SignalDisconnected).await });
            assert!(
                tokio::time::timeout(Duration::from_millis(200), &mut restart).await.is_err(),
                "restart should still be tearing the old transport down"
            );

            // The caller cancels the resume and releases the session. The resume's own task is
            // detached by then, so it completes regardless.
            drop(restart);
            drop(connected);
            STALL_CLOSE.store(false, Ordering::SeqCst);

            eventually(
                || LATEST_CONN.lock().unwrap().is_some(),
                "the resume never opened a transport",
            )
            .await;
            assert_released(take_conn()).await;
            drop(first);
        });
    }

    #[test]
    fn close_wakes_a_parked_next_event() {
        with_mock_transport(async {
            let connected = connect().await;
            take_conn();

            let events = connected.events.clone();
            let parked = crate::runtime::runtime().spawn(async move { events.next_event().await });
            tokio::time::sleep(Duration::from_millis(100)).await;

            connected.session.close().await;

            assert!(
                tokio::time::timeout(Duration::from_secs(5), parked)
                    .await
                    .expect("close should wake the parked call")
                    .expect("the task should not panic")
                    .is_none(),
                "a parked next_event should end once the session is closed"
            );
        });
    }

    #[test]
    fn a_refused_upgrade_surfaces_as_a_handshake_error() {
        with_mock_transport(async {
            FAIL_CONNECT.store(true, Ordering::SeqCst);
            let Err(error) =
                connect_signal_session(URL.to_string(), TOKEN.to_string(), options(), None).await
            else {
                panic!("a refused upgrade cannot produce a session");
            };
            assert!(
                matches!(error, SignalError::Handshake { status: 503 }),
                "expected a handshake error, got {error:?}"
            );
        });
    }
}
