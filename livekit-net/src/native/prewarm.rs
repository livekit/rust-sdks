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

//! Native backend of [`crate::PrewarmedConnection`].

use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use tokio::{io::AsyncReadExt, net::TcpStream, sync::oneshot};
use tokio_tungstenite::{tungstenite, MaybeTlsStream};

use crate::{Header, TransportError, WsConnectResult};

pub(super) type Stream = MaybeTlsStream<TcpStream>;

/// Upper bound on the WebSocket upgrade over a pre-warmed stream. A close (FIN)
/// or reset (RST) is seen by the watcher or fails the upgrade at once; this only
/// fires when a middlebox dropped the stream silently, which never fails on its
/// own. Kept well above a normal upgrade (~1 RTT), as giving up on a slow but
/// live server makes the fresh dial send a second join for the same identity.
const UPGRADE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Upper bound on opening a pre-warmed stream (TCP connect plus TLS handshake),
/// matching the default signal connect timeout.
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

/// The server a stream is connected to. A pre-warmed stream is only used for
/// the exact scheme, host and port it was opened for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    tls: bool,
    host: String,
    port: u16,
}

impl TryFrom<&url::Url> for Endpoint {
    type Error = TransportError;

    fn try_from(url: &url::Url) -> Result<Self, Self::Error> {
        // Signalling rewrites http(s) to ws(s) before connecting, so both
        // spellings of a URL must match.
        let tls = match url.scheme() {
            "wss" | "https" => true,
            "ws" | "http" => false,
            other => {
                return Err(TransportError::Connection(format!("unsupported scheme: {other}")))
            }
        };
        let host = match url.host() {
            Some(url::Host::Domain(domain)) => domain.to_owned(),
            Some(url::Host::Ipv4(ip)) => ip.to_string(),
            Some(url::Host::Ipv6(ip)) => ip.to_string(),
            None => return Err(TransportError::Connection("URL has no host".into())),
        };
        let port = url.port().unwrap_or(if tls { 443 } else { 80 });
        Ok(Self { tls, host, port })
    }
}

/// Asks the watcher for its stream.
type TakeTx = oneshot::Sender<oneshot::Sender<Stream>>;

/// An open stream waiting for its WebSocket upgrade.
pub(crate) struct Warm {
    endpoint: Endpoint,
    /// `None` once the stream was taken or the connection closed. Dropping it
    /// stops the watcher, which owns the stream.
    take_tx: Mutex<Option<TakeTx>>,
}

impl Warm {
    pub(crate) async fn open(url: &str) -> Result<Self, TransportError> {
        Self::open_with_timeout(url, DIAL_TIMEOUT).await
    }

    async fn open_with_timeout(url: &str, timeout: Duration) -> Result<Self, TransportError> {
        let url = url::Url::parse(url).map_err(|e| TransportError::Connection(e.to_string()))?;
        let endpoint = Endpoint::try_from(&url)?;
        if super::proxy::proxy_for(endpoint.tls).is_some() {
            // A direct stream would bypass the proxy the connect path uses.
            return Err(TransportError::Other("prewarm is not supported through a proxy".into()));
        }

        let stream = tokio::time::timeout(timeout, dial(&endpoint))
            .await
            .map_err(|_| TransportError::Timeout)??;
        let (take_tx, take_rx) = oneshot::channel();
        tokio::spawn(Watcher { take_rx, stream }.run());
        log::debug!("pre-warmed connection to {}", crate::redact_url(&url));
        Ok(Self { endpoint, take_tx: Mutex::new(Some(take_tx)) })
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.take_tx().as_ref().is_some_and(|tx| !tx.is_closed())
    }

    pub(crate) fn close(&self) {
        let take_tx = self.take_tx().take();
        drop(take_tx);
    }

    /// Run the WebSocket upgrade for `url` over the pre-warmed stream.
    ///
    /// Returns `Ok(None)` when the stream cannot be used (already used, closed,
    /// opened for another server, or the upgrade failed or timed out), in which
    /// case the caller should dial a fresh connection. An HTTP error status from
    /// the server is the server's real answer and is returned as an error. The
    /// stream is consumed either way.
    pub(crate) async fn upgrade(
        &self,
        url: &str,
        headers: &[Header],
        timeout: Duration,
    ) -> Result<Option<WsConnectResult>, TransportError> {
        let parsed = url::Url::parse(url).map_err(|e| TransportError::Connection(e.to_string()))?;
        let Some(take_tx) = self.take_tx().take() else { return Ok(None) };
        if Endpoint::try_from(&parsed).ok().as_ref() != Some(&self.endpoint) {
            log::debug!("pre-warmed connection is for another server, dialing a fresh one");
            return Ok(None);
        }
        if super::proxy::proxy_for(self.endpoint.tls).is_some() {
            return Ok(None);
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        // Both fail if the watcher already exited because the server closed the stream.
        if take_tx.send(reply_tx).is_err() {
            return Ok(None);
        }
        let Ok(stream) = reply_rx.await else { return Ok(None) };

        let request = super::build_request(&parsed, headers)?;
        let upgrade = tokio_tungstenite::client_async_with_config(request, stream, None);
        match tokio::time::timeout(timeout.min(UPGRADE_TIMEOUT), upgrade).await {
            Ok(Ok((ws, _))) => {
                log::debug!("using pre-warmed connection to {}", crate::redact_url(&parsed));
                Ok(Some(WsConnectResult {
                    connection: Arc::new(super::connection::NativeConnection::new(ws)),
                }))
            }
            Ok(Err(e @ tungstenite::Error::Http(_))) => Err(super::proxy::map_ws_err(e)),
            Ok(Err(e)) => {
                log::warn!("pre-warmed connection failed, dialing a fresh one: {e}");
                Ok(None)
            }
            Err(_) => {
                log::warn!("pre-warmed connection timed out, dialing a fresh one");
                Ok(None)
            }
        }
    }

    fn take_tx(&self) -> MutexGuard<'_, Option<TakeTx>> {
        // Every critical section is a single `Option` operation.
        self.take_tx.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Owns the stream until it is taken, the server closes it, or its [`Warm`] is dropped.
struct Watcher {
    take_rx: oneshot::Receiver<oneshot::Sender<Stream>>,
    stream: Stream,
}

impl Watcher {
    async fn run(self) {
        let Self { take_rx, mut stream } = self;
        let mut buf = [0u8; 64];
        tokio::select! {
            // `Err` means the connection was closed or dropped.
            reply_tx = take_rx => {
                if let Ok(reply_tx) = reply_tx {
                    let _ = reply_tx.send(stream);
                }
            }
            // The server sends nothing before the upgrade request, so any
            // application data, EOF, or error means the stream is unusable.
            // TLS 1.3 session tickets are consumed by the TLS layer and never
            // complete this read.
            _ = stream.read(&mut buf) => log::debug!("pre-warmed connection closed by the server"),
        }
    }
}

async fn dial(endpoint: &Endpoint) -> Result<Stream, TransportError> {
    let tcp = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|e| TransportError::Connection(e.to_string()))?;
    let _ = tcp.set_nodelay(true);
    if !endpoint.tls {
        return Ok(MaybeTlsStream::Plain(tcp));
    }
    tls::connect(&endpoint.host, tcp).await
}

/// TLS for pre-warmed streams, mirroring the backend tokio-tungstenite picks when
/// it is given no connector: native-tls when enabled, otherwise rustls.
mod tls {
    use super::Stream;
    use crate::TransportError;
    use tokio::net::TcpStream;

    #[cfg(any(feature = "native-tls", feature = "native-tls-vendored"))]
    pub(super) async fn connect(host: &str, tcp: TcpStream) -> Result<Stream, TransportError> {
        use tokio_tungstenite::MaybeTlsStream;

        let connector = native_tls::TlsConnector::new()
            .map_err(|e| TransportError::Connection(e.to_string()))?;
        let stream = tokio_native_tls::TlsConnector::from(connector)
            .connect(host, tcp)
            .await
            .map_err(|e| TransportError::Connection(format!("TLS connection error: {e}")))?;
        Ok(MaybeTlsStream::NativeTls(stream))
    }

    #[cfg(all(
        not(any(feature = "native-tls", feature = "native-tls-vendored")),
        any(
            feature = "rustls-tls-native-roots",
            feature = "rustls-tls-webpki-roots",
            feature = "__rustls-tls"
        )
    ))]
    pub(super) async fn connect(host: &str, tcp: TcpStream) -> Result<Stream, TransportError> {
        use tokio_rustls::{rustls::pki_types::ServerName, TlsConnector};
        use tokio_tungstenite::MaybeTlsStream;

        let server_name = ServerName::try_from(host.to_owned())
            .map_err(|_| TransportError::Connection(format!("Invalid DNS name: {host}")))?;
        let stream = TlsConnector::from(rustls_config()?)
            .connect(server_name, tcp)
            .await
            .map_err(|e| TransportError::Connection(format!("TLS connection error: {e}")))?;
        Ok(MaybeTlsStream::Rustls(stream))
    }

    /// The client config, built once: loading native roots is slow, and a
    /// shared config also shares its TLS session cache.
    #[cfg(all(
        not(any(feature = "native-tls", feature = "native-tls-vendored")),
        any(
            feature = "rustls-tls-native-roots",
            feature = "rustls-tls-webpki-roots",
            feature = "__rustls-tls"
        )
    ))]
    fn rustls_config() -> Result<std::sync::Arc<tokio_rustls::rustls::ClientConfig>, TransportError>
    {
        use std::sync::{Arc, OnceLock};
        use tokio_rustls::rustls::{ClientConfig, RootCertStore};

        static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
        if let Some(config) = CONFIG.get() {
            return Ok(Arc::clone(config));
        }

        #[allow(unused_mut)]
        let mut roots = RootCertStore::empty();
        #[cfg(feature = "rustls-tls-native-roots")]
        {
            let result = rustls_native_certs::load_native_certs();
            if !result.errors.is_empty() {
                log::warn!("native root CA certificate loading errors: {:?}", result.errors);
            }
            roots.add_parsable_certificates(result.certs);
        }
        #[cfg(feature = "rustls-tls-webpki-roots")]
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        if roots.is_empty() {
            return Err(TransportError::Connection("no root CA certificates available".into()));
        }

        let config =
            Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        Ok(Arc::clone(CONFIG.get_or_init(|| config)))
    }

    #[cfg(not(any(
        feature = "native-tls",
        feature = "native-tls-vendored",
        feature = "rustls-tls-native-roots",
        feature = "rustls-tls-webpki-roots",
        feature = "__rustls-tls"
    )))]
    pub(super) async fn connect(_host: &str, _tcp: TcpStream) -> Result<Stream, TransportError> {
        Err(TransportError::Connection("TLS support is not enabled".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::AsyncWriteExt, net::TcpListener};

    async fn listener() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        (listener, url)
    }

    async fn wait_until(cond: impl Fn() -> bool) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !cond() {
            assert!(tokio::time::Instant::now() < deadline, "condition not met in time");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn assert_eof(server: &mut TcpStream) {
        let mut buf = [0u8; 1];
        let n = tokio::time::timeout(Duration::from_secs(2), server.read(&mut buf)).await;
        assert!(matches!(n, Ok(Ok(0))), "server side did not see the stream close");
    }

    #[test]
    fn endpoint_normalizes_scheme_and_port() {
        let parse = |s: &str| Endpoint::try_from(&url::Url::parse(s).unwrap()).unwrap();
        assert_eq!(parse("https://example.com"), parse("wss://example.com:443/rtc/v1?x=1"));
        assert_eq!(parse("http://example.com:7880"), parse("ws://example.com:7880/rtc"));
        assert_ne!(parse("ws://example.com"), parse("wss://example.com"));
        assert_eq!(parse("ws://[::1]:7880").host, "::1");
    }

    #[tokio::test]
    async fn server_close_marks_dead() {
        let (listener, url) = listener().await;
        let warm = Warm::open(&url).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        assert!(warm.is_alive());

        drop(server);
        wait_until(|| !warm.is_alive()).await;
        let used = warm.upgrade(&url, &[], Duration::from_secs(1)).await.unwrap();
        assert!(used.is_none());
    }

    #[tokio::test]
    async fn drop_closes_the_stream() {
        let (listener, url) = listener().await;
        let warm = Warm::open(&url).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();

        drop(warm);
        assert_eof(&mut server).await;
    }

    #[tokio::test]
    async fn close_closes_the_stream() {
        let (listener, url) = listener().await;
        let warm = Warm::open(&url).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();

        warm.close();
        assert!(!warm.is_alive());
        assert_eof(&mut server).await;
    }

    #[tokio::test]
    async fn other_server_is_not_used_and_closes() {
        let (listener, url) = listener().await;
        let warm = Warm::open(&url).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();

        let used = warm.upgrade("ws://127.0.0.1:1", &[], Duration::from_secs(1)).await.unwrap();
        assert!(used.is_none());
        assert!(!warm.is_alive());
        assert_eof(&mut server).await;
    }

    #[tokio::test]
    async fn silent_server_times_out() {
        let (listener, url) = listener().await;
        let warm = Warm::open(&url).await.unwrap();
        // Accept but never answer the upgrade, like a silently dropped stream.
        let (mut server, _) = listener.accept().await.unwrap();

        let started = tokio::time::Instant::now();
        let used = warm.upgrade(&url, &[], Duration::from_millis(200)).await.unwrap();
        assert!(used.is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = server.shutdown().await;
    }

    #[cfg(any(
        feature = "native-tls",
        feature = "native-tls-vendored",
        feature = "rustls-tls-native-roots",
        feature = "rustls-tls-webpki-roots",
        feature = "__rustls-tls"
    ))]
    #[tokio::test]
    async fn stalled_tls_handshake_times_out() {
        let (listener, _) = listener().await;
        let url = format!("wss://{}", listener.local_addr().unwrap());
        // Accept TCP but never answer the ClientHello.
        let accept = tokio::spawn(async move { listener.accept().await.unwrap() });

        let started = tokio::time::Instant::now();
        let result = Warm::open_with_timeout(&url, Duration::from_millis(200)).await;
        assert!(matches!(result, Err(TransportError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(accept.await.unwrap());
    }
}
