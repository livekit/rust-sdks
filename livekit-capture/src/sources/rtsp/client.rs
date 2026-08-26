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

//! RTSP/1.0 client over TCP: request writing, response reading, and the
//! interleaved (`$`-framed) stream demuxer.

use std::{
    fmt,
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    str,
    time::{Duration, Instant},
};

use bytes::{Buf, Bytes, BytesMut};
use rtsp_types::{headers, HeaderName, Message, Method, ParseError, Url, Version};

use super::{auth::RtspAuthContext, auth::RtspCredentials, RtspPhase, RtspVideoSourceError};

/// Socket read timeout: the poll granularity for the stop token, keepalives,
/// and deadlines, kept near one frame interval per the pump contract.
const READ_POLL: Duration = Duration::from_millis(100);

/// Socket write timeout; requests are small, so a stalled write means the
/// connection is gone.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Upper bound on one buffered-but-incomplete RTSP message. Interleaved
/// frames are at most 4 + 65535 bytes, and responses are far smaller, so an
/// incomplete message larger than this means the framing is corrupt.
const MAX_PENDING_MESSAGE_BYTES: usize = 128 * 1024;

/// Bytes requested from the socket per read.
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// A parsed `rtsp://` or `rtsps://` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RtspUrl {
    /// Request URI with any userinfo stripped, so credentials never appear
    /// on the wire outside the `Authorization` header.
    pub(super) request_uri: String,
    /// Credentials from the URL userinfo, percent-decoded.
    pub(super) credentials: Option<RtspCredentials>,
    /// Whether the URL requires TLS (`rtsps://`).
    pub(super) tls: bool,
    connect_host: String,
    port: u16,
}

impl RtspUrl {
    /// Parses an `rtsp(s)://[user:password@]host[:port][/path]` URL.
    pub(super) fn parse(url: &str) -> Result<Self, RtspVideoSourceError> {
        let parsed =
            Url::parse(url).map_err(|_| RtspVideoSourceError::InvalidUrl("malformed URL"))?;
        let tls = match parsed.scheme() {
            "rtsp" => false,
            "rtsps" => true,
            _ => {
                return Err(RtspVideoSourceError::InvalidUrl(
                    "expected rtsp:// or rtsps:// scheme",
                ))
            }
        };

        // `Host::Ipv6` renders unbracketed, as `ToSocketAddrs` expects.
        let connect_host = match parsed.host() {
            Some(url::Host::Domain(domain)) if !domain.is_empty() => domain.to_owned(),
            Some(url::Host::Ipv4(address)) => address.to_string(),
            Some(url::Host::Ipv6(address)) => address.to_string(),
            _ => return Err(RtspVideoSourceError::InvalidUrl("missing host")),
        };
        let port = parsed.port().unwrap_or(if tls { 322 } else { 554 });

        let credentials = if parsed.username().is_empty() && parsed.password().is_none() {
            None
        } else {
            let username = percent_decode(parsed.username());
            if username.is_empty() {
                return Err(RtspVideoSourceError::InvalidUrl("missing username"));
            }
            let password = parsed.password().map(percent_decode).unwrap_or_default();
            Some(RtspCredentials { username, password })
        };

        // Strip userinfo so credentials never appear on the wire outside
        // the `Authorization` header.
        let mut request_uri = parsed;
        let _ = request_uri.set_username("");
        let _ = request_uri.set_password(None);

        Ok(Self { request_uri: request_uri.to_string(), credentials, tls, connect_host, port })
    }
}

/// Decodes RFC 3986 percent-escapes; malformed escapes pass through as-is.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        let escape = (bytes[cursor] == b'%')
            .then(|| bytes.get(cursor + 1..cursor + 3))
            .flatten()
            .and_then(|digits| str::from_utf8(digits).ok())
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        if let Some(byte) = escape {
            decoded.push(byte);
            cursor += 3;
        } else {
            decoded.push(bytes[cursor]);
            cursor += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// A parsed RTSP response.
#[derive(Debug, Clone)]
pub(super) struct RtspResponse {
    pub(super) status_code: u16,
    pub(super) reason: String,
    headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
}

impl RtspResponse {
    /// Converts a parsed `rtsp-types` response into the client's view.
    fn from_message(response: rtsp_types::Response<Vec<u8>>) -> Self {
        Self {
            status_code: response.status().into(),
            reason: response.reason_phrase().to_owned(),
            headers: response
                .headers()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_str().to_owned()))
                .collect(),
            body: response.into_body(),
        }
    }

    pub(super) fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    /// Returns the first header with the given name, case-insensitively.
    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Returns every header with the given name, case-insensitively.
    pub(super) fn headers<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// One unit read from the interleaved stream.
#[derive(Debug)]
pub(super) enum InterleavedPoll {
    /// An interleaved binary frame.
    Frame {
        /// Interleaved channel the frame arrived on.
        channel: u8,
        /// Frame payload, without the 4-byte interleaved header.
        payload: Bytes,
    },
    /// An in-stream RTSP response, such as a keepalive reply.
    Response(RtspResponse),
    /// A read timed out; framing state is preserved for the next poll.
    TimedOut,
    /// The stream ended cleanly at a unit boundary.
    EndOfStream,
}

/// Result of one attempt to read more stream bytes.
enum StreamFill {
    Filled,
    Eof,
    TimedOut,
}

/// The connection's byte stream: plain TCP, or TLS over TCP for `rtsps://`.
enum Transport {
    Plain(TcpStream),
    #[cfg(feature = "source-rtsp-tls")]
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Transport {
    /// The underlying TCP socket, whose timeouts bound every read and write.
    fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(stream) => stream,
            #[cfg(feature = "source-rtsp-tls")]
            Self::Tls(stream) => stream.get_ref(),
        }
    }
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            #[cfg(feature = "source-rtsp-tls")]
            Self::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buf),
            #[cfg(feature = "source-rtsp-tls")]
            Self::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            #[cfg(feature = "source-rtsp-tls")]
            Self::Tls(stream) => stream.flush(),
        }
    }
}

/// RTSP connection: owns the transport stream, the read buffer, the request
/// sequence number, and the authentication context.
pub(super) struct RtspClient {
    stream: Transport,
    buf: BytesMut,
    scratch: Vec<u8>,
    cseq: u32,
    auth: RtspAuthContext,
    last_read_at: Instant,
    logged_server_request: bool,
}

// Manual so the read buffer's contents are not dumped; the authentication
// context redacts its own credentials.
impl fmt::Debug for RtspClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtspClient")
            .field("cseq", &self.cseq)
            .finish_non_exhaustive()
    }
}

impl RtspClient {
    /// Connects to the URL's host, bounded by `deadline`, establishes TLS
    /// for `rtsps://` URLs, and prepares the socket for polled reads.
    pub(super) fn connect(
        url: &RtspUrl,
        credentials: Option<RtspCredentials>,
        accept_invalid_tls_certs: bool,
        deadline: Instant,
    ) -> Result<Self, RtspVideoSourceError> {
        #[cfg(not(feature = "source-rtsp-tls"))]
        let _ = accept_invalid_tls_certs;

        let addrs = (url.connect_host.as_str(), url.port).to_socket_addrs()?;
        let mut last_error = None;
        let mut stream = None;
        for addr in addrs {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()) else {
                break;
            };
            match TcpStream::connect_timeout(&addr, remaining) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(err) => last_error = Some(err),
            }
        }
        let Some(stream) = stream else {
            return Err(match last_error {
                Some(err) => RtspVideoSourceError::Io(err),
                None => RtspVideoSourceError::Timeout { phase: RtspPhase::Connect },
            });
        };

        if let Err(err) = stream.set_nodelay(true) {
            log::debug!("failed to disable Nagle's algorithm on the RTSP stream: {err}");
        }
        stream.set_read_timeout(Some(READ_POLL))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;

        let stream = match url.tls {
            false => Transport::Plain(stream),
            #[cfg(feature = "source-rtsp-tls")]
            true => Transport::Tls(Box::new(tls::establish(
                stream,
                &url.connect_host,
                accept_invalid_tls_certs,
                deadline,
            )?)),
            #[cfg(not(feature = "source-rtsp-tls"))]
            true => return Err(RtspVideoSourceError::TlsNotSupported),
        };

        Ok(Self {
            stream,
            buf: BytesMut::with_capacity(READ_CHUNK_BYTES),
            scratch: vec![0; READ_CHUNK_BYTES],
            cseq: 1,
            auth: RtspAuthContext::new(credentials),
            last_read_at: Instant::now(),
            logged_server_request: false,
        })
    }

    /// Sends a request and reads its response, bounded by `deadline`,
    /// retrying once with credentials on a 401 challenge. Non-2xx statuses
    /// become [`RtspVideoSourceError::RtspStatus`].
    pub(super) fn request(
        &mut self,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        deadline: Instant,
        phase: RtspPhase,
    ) -> Result<RtspResponse, RtspVideoSourceError> {
        self.write_request(method, uri, headers)?;
        let mut response = self.read_response(deadline, phase)?;
        if response.status_code == 401 {
            self.auth.update_from_unauthorized(&response)?;
            self.write_request(method, uri, headers)?;
            response = self.read_response(deadline, phase)?;
        }

        if !response.is_success() {
            return Err(RtspVideoSourceError::RtspStatus {
                code: response.status_code,
                reason: response.reason,
            });
        }
        Ok(response)
    }

    /// Writes a request as a single buffered write, without waiting for the
    /// response. Used for keepalives and TEARDOWN, whose replies (if any)
    /// arrive in-band.
    pub(super) fn write_request(
        &mut self,
        method: &str,
        uri: &str,
        extra_headers: &[(&str, &str)],
    ) -> Result<(), RtspVideoSourceError> {
        let cseq = self.cseq;
        self.cseq = self.cseq.saturating_add(1);
        let authorization = self.auth.header(method, uri)?;
        let request_uri = Url::parse(uri)
            .map_err(|_| RtspVideoSourceError::InvalidUrl("request URI is not a valid URL"))?;

        let mut request = rtsp_types::Request::builder(request_method(method), Version::V1_0)
            .request_uri(request_uri)
            .header(headers::CSEQ, cseq.to_string())
            .header(headers::USER_AGENT, "livekit-capture/0.1".to_owned());
        if let Some(authorization) = authorization {
            request = request.header(headers::AUTHORIZATION, authorization);
        }
        for (name, value) in extra_headers {
            // Header names come from this crate's own call sites.
            let name = HeaderName::try_from(*name).expect("static header names are valid");
            request = request.header(name, (*value).to_owned());
        }

        let mut bytes = Vec::with_capacity(256);
        request
            .empty()
            .write(&mut bytes)
            .map_err(|err| RtspVideoSourceError::Io(io::Error::other(err)))?;
        self.stream.write_all(&bytes)?;
        self.stream.flush()?;
        Ok(())
    }

    /// Reads one RTSP response from the stream, bounded by `deadline`.
    fn read_response(
        &mut self,
        deadline: Instant,
        phase: RtspPhase,
    ) -> Result<RtspResponse, RtspVideoSourceError> {
        loop {
            if !self.buf.is_empty() {
                match Message::parse(&self.buf) {
                    Ok((message, consumed)) => {
                        self.buf.advance(consumed);
                        match message {
                            Message::Response(response) => {
                                return Ok(RtspResponse::from_message(response));
                            }
                            Message::Data(_) | Message::Request(_) => {
                                return Err(RtspVideoSourceError::InvalidResponse(
                                    "expected a response",
                                ));
                            }
                        }
                    }
                    Err(ParseError::Incomplete(_)) => self.check_pending_size()?,
                    Err(ParseError::Error) => {
                        return Err(RtspVideoSourceError::InvalidResponse("malformed response"));
                    }
                }
            }
            match self.fill()? {
                StreamFill::Filled => {}
                StreamFill::Eof => {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
                StreamFill::TimedOut => {
                    if Instant::now() >= deadline {
                        return Err(RtspVideoSourceError::Timeout { phase });
                    }
                }
            }
        }
    }

    /// Fails when a still-incomplete message exceeds the framing bound.
    fn check_pending_size(&self) -> Result<(), RtspVideoSourceError> {
        if self.buf.len() > MAX_PENDING_MESSAGE_BYTES {
            return Err(RtspVideoSourceError::InvalidResponse("message too large"));
        }
        Ok(())
    }

    /// Reads the next interleaved unit, returning within roughly one
    /// [`READ_POLL`] when the stream is silent. Framing state survives
    /// timed-out reads.
    pub(super) fn poll_unit(&mut self) -> Result<InterleavedPoll, RtspVideoSourceError> {
        loop {
            if let Some(unit) = self.parse_front()? {
                return Ok(unit);
            }
            match self.fill()? {
                StreamFill::Filled => {}
                StreamFill::Eof => {
                    if self.buf.is_empty() {
                        return Ok(InterleavedPoll::EndOfStream);
                    }
                    // The stream ended inside an interleaved unit.
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
                StreamFill::TimedOut => return Ok(InterleavedPoll::TimedOut),
            }
        }
    }

    /// Time since stream bytes last arrived, for the idle limit.
    pub(super) fn idle_for(&self) -> Duration {
        self.last_read_at.elapsed()
    }

    /// Parses one complete unit from the front of the buffer.
    fn parse_front(&mut self) -> Result<Option<InterleavedPoll>, RtspVideoSourceError> {
        loop {
            if self.buf.is_empty() {
                return Ok(None);
            }
            match Message::parse(&self.buf) {
                Ok((message, consumed)) => {
                    self.buf.advance(consumed);
                    match message {
                        Message::Data(data) => {
                            let channel = data.channel_id();
                            let payload = Bytes::from(data.into_body());
                            return Ok(Some(InterleavedPoll::Frame { channel, payload }));
                        }
                        Message::Response(response) => {
                            return Ok(Some(InterleavedPoll::Response(
                                RtspResponse::from_message(response),
                            )));
                        }
                        Message::Request(request) => {
                            // Some servers send requests (ANNOUNCE, keepalive
                            // checks) to the client mid-stream; ignore them.
                            if !self.logged_server_request {
                                self.logged_server_request = true;
                                log::warn!(
                                    "ignoring in-band RTSP {:?} request from the server",
                                    request.method(),
                                );
                            }
                        }
                    }
                }
                Err(ParseError::Incomplete(_)) => {
                    self.check_pending_size()?;
                    return Ok(None);
                }
                Err(ParseError::Error) => return Err(RtspVideoSourceError::UnexpectedData),
            }
        }
    }

    /// Reads more stream bytes into the buffer.
    fn fill(&mut self) -> Result<StreamFill, RtspVideoSourceError> {
        loop {
            match self.stream.read(&mut self.scratch) {
                Ok(0) => return Ok(StreamFill::Eof),
                Ok(read) => {
                    self.buf.extend_from_slice(&self.scratch[..read]);
                    self.last_read_at = Instant::now();
                    return Ok(StreamFill::Filled);
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) if is_timeout_io_error(&err) => return Ok(StreamFill::TimedOut),
                // TLS peers that drop the connection without a close_notify
                // (most cameras) surface as UnexpectedEof; treat it like a
                // plain EOF and let the caller decide whether the framing
                // was left mid-unit.
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(StreamFill::Eof)
                }
                Err(err) => return Err(err.into()),
            }
        }
    }
}

/// TLS support for `rtsps://` URLs.
#[cfg(feature = "source-rtsp-tls")]
mod tls {
    use std::{io, net::TcpStream, sync::Arc, time::Instant};

    use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
    use rustls_pki_types::ServerName;

    use super::{is_timeout_io_error, RtspPhase, RtspVideoSourceError};

    /// Establishes TLS over a connected TCP stream, driving the handshake to
    /// completion bounded by `deadline`.
    ///
    /// The handshake must finish here: afterwards, writes never need to
    /// read, so the request path's timeout handling stays valid. During the
    /// handshake, the socket's read timeout is the retry granularity.
    pub(super) fn establish(
        stream: TcpStream,
        host: &str,
        accept_invalid_certs: bool,
        deadline: Instant,
    ) -> Result<StreamOwned<ClientConnection, TcpStream>, RtspVideoSourceError> {
        let config = client_config(accept_invalid_certs)?;
        // `ServerName` accepts both DNS names and the IP literals cameras
        // are usually addressed by.
        let server_name = ServerName::try_from(host.to_owned())
            .map_err(|err| RtspVideoSourceError::Tls(err.to_string()))?;
        let mut connection = ClientConnection::new(Arc::new(config), server_name)
            .map_err(|err| RtspVideoSourceError::Tls(err.to_string()))?;

        let mut stream = stream;
        while connection.is_handshaking() {
            match connection.complete_io(&mut stream) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) if is_timeout_io_error(&err) => {
                    if Instant::now() >= deadline {
                        return Err(RtspVideoSourceError::Timeout { phase: RtspPhase::Connect });
                    }
                }
                // rustls reports TLS-level handshake failures as InvalidData.
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                    return Err(RtspVideoSourceError::Tls(err.to_string()));
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(StreamOwned::new(connection, stream))
    }

    fn client_config(accept_invalid_certs: bool) -> Result<ClientConfig, RtspVideoSourceError> {
        if accept_invalid_certs {
            return Ok(ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(danger::NoVerification))
                .with_no_client_auth());
        }

        let mut roots = RootCertStore::empty();
        // Individually unparsable certificates in the OS store are skipped;
        // an entirely unavailable store is an error.
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            return Err(RtspVideoSourceError::Tls(
                "no usable system root certificates".to_owned(),
            ));
        }
        Ok(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
    }

    /// Certificate "verification" that accepts anything; see
    /// [`RtspVideoSourceConfig::accept_invalid_tls_certs`](super::super::RtspVideoSourceConfig::accept_invalid_tls_certs).
    mod danger {
        use rustls::{
            client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
            crypto::{ring, verify_tls12_signature, verify_tls13_signature},
            DigitallySignedStruct, Error, SignatureScheme,
        };
        use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

        #[derive(Debug)]
        pub(super) struct NoVerification;

        impl ServerCertVerifier for NoVerification {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                message: &[u8],
                cert: &CertificateDer<'_>,
                dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, Error> {
                verify_tls12_signature(
                    message,
                    cert,
                    dss,
                    &ring::default_provider().signature_verification_algorithms,
                )
            }

            fn verify_tls13_signature(
                &self,
                message: &[u8],
                cert: &CertificateDer<'_>,
                dss: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, Error> {
                verify_tls13_signature(
                    message,
                    cert,
                    dss,
                    &ring::default_provider().signature_verification_algorithms,
                )
            }

            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                ring::default_provider().signature_verification_algorithms.supported_schemes()
            }
        }
    }
}

fn is_timeout_io_error(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

/// Maps a request method from this crate's own call sites to its typed form.
fn request_method(method: &str) -> Method {
    match method {
        "DESCRIBE" => Method::Describe,
        "SETUP" => Method::Setup,
        "PLAY" => Method::Play,
        "OPTIONS" => Method::Options,
        "TEARDOWN" => Method::Teardown,
        other => Method::Extension(other.to_owned()),
    }
}

#[cfg(test)]
impl RtspResponse {
    /// Parses one complete response, for tests in sibling modules.
    pub(super) fn parse_for_tests(bytes: &[u8]) -> Self {
        match Message::parse(bytes).expect("invalid response") {
            (Message::Response(response), _) => Self::from_message(response),
            (other, _) => panic!("expected a response, got {other:?}"),
        }
    }
}

/// Extracts the session identifier from a `Session` header value.
pub(super) fn parse_session_id(session_header: &str) -> Result<String, RtspVideoSourceError> {
    let session_id = session_header.split(';').next().unwrap_or_default().trim();
    if session_id.is_empty() {
        return Err(RtspVideoSourceError::InvalidResponse("empty session id"));
    }
    Ok(session_id.to_owned())
}

/// Extracts the `timeout` parameter of a `Session` header value.
pub(super) fn parse_session_timeout_secs(session_header: &str) -> Option<u64> {
    session_header.split(';').skip(1).find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name.trim().eq_ignore_ascii_case("timeout") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

/// Extracts the RTP channel from a SETUP response's `Transport` header.
pub(super) fn parse_interleaved_channel(
    transport_header: Option<&str>,
) -> Result<u8, RtspVideoSourceError> {
    let transport_header =
        transport_header.ok_or(RtspVideoSourceError::MissingHeader("Transport"))?;
    for part in transport_header.split(';') {
        let Some(value) = part.trim().strip_prefix("interleaved=") else {
            continue;
        };
        if let Some(first) = value.split('-').next().and_then(|channel| channel.parse().ok()) {
            return Ok(first);
        }
    }
    Err(RtspVideoSourceError::InvalidResponse("Transport header without interleaved channels"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_credentials_but_strips_them_from_request_uri() {
        let url = RtspUrl::parse("rtsp://admin:secret@camera.example:554/live").unwrap();

        assert_eq!(url.request_uri, "rtsp://camera.example:554/live");
        assert_eq!(
            url.credentials,
            Some(RtspCredentials { username: "admin".to_owned(), password: "secret".to_owned() })
        );
    }

    #[test]
    fn percent_decodes_userinfo() {
        let url = RtspUrl::parse("rtsp://user%40lk:p%40ss%2Fword@camera.example/live").unwrap();
        assert_eq!(
            url.credentials,
            Some(RtspCredentials {
                username: "user@lk".to_owned(),
                password: "p@ss/word".to_owned(),
            })
        );
    }

    #[test]
    fn parses_rtsps_urls() {
        let url = RtspUrl::parse("rtsps://camera.example/live").unwrap();
        assert!(url.tls);
        assert_eq!(url.port, 322);
        assert_eq!(url.request_uri, "rtsps://camera.example/live");

        let url = RtspUrl::parse("rtsps://admin:secret@camera.example:7441/live").unwrap();
        assert!(url.tls);
        assert_eq!(url.port, 7441);
        assert_eq!(url.request_uri, "rtsps://camera.example:7441/live");
        assert!(url.credentials.is_some());

        assert!(!RtspUrl::parse("rtsp://camera.example/live").unwrap().tls);
    }

    #[test]
    fn defaults_to_port_554() {
        let url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        assert_eq!(url.port, 554);
        assert_eq!(url.request_uri, "rtsp://camera.example/live");
    }

    #[test]
    fn parses_bracketed_ipv6_host() {
        let url = RtspUrl::parse("rtsp://[2001:db8::1]:8554/live").unwrap();
        assert_eq!(url.connect_host, "2001:db8::1");
        assert_eq!(url.port, 8554);
        assert_eq!(url.request_uri, "rtsp://[2001:db8::1]:8554/live");
    }

    #[test]
    fn rejects_invalid_urls() {
        assert!(matches!(
            RtspUrl::parse("http://camera.example/live"),
            Err(RtspVideoSourceError::InvalidUrl(_))
        ));
        assert!(matches!(
            RtspUrl::parse("rtsp:///live"),
            Err(RtspVideoSourceError::InvalidUrl(_))
        ));
        assert!(matches!(
            RtspUrl::parse("rtsp://:secret@camera.example/live"),
            Err(RtspVideoSourceError::InvalidUrl(_))
        ));
        assert!(matches!(
            RtspUrl::parse("rtsp://camera.example:notaport/live"),
            Err(RtspVideoSourceError::InvalidUrl(_))
        ));
    }

    #[test]
    fn rejects_oversized_incomplete_message() {
        use std::{net::TcpListener, thread};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // Claim a body far larger than the framing bound, stream past
            // the bound without completing it, then stall.
            stream
                .write_all(b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nContent-Length: 999999\r\n\r\n")
                .unwrap();
            stream.write_all(&vec![b'a'; MAX_PENDING_MESSAGE_BYTES + 8 * 1024]).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(200));
        });

        let url = RtspUrl::parse(&format!("rtsp://{addr}/test")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut client = RtspClient::connect(&url, None, false, deadline).unwrap();
        let err = client
            .request("DESCRIBE", &url.request_uri, &[], deadline, RtspPhase::Describe)
            .unwrap_err();

        assert!(
            matches!(err, RtspVideoSourceError::InvalidResponse("message too large")),
            "unexpected error: {err:?}"
        );
        server.join().unwrap();
    }

    #[test]
    fn converts_parsed_responses() {
        let response = RtspResponse::parse_for_tests(
            b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nContent-Length: 4\r\n\r\nbody",
        );

        assert_eq!(response.status_code, 200);
        assert_eq!(response.reason, "OK");
        assert_eq!(response.header("cseq"), Some("1"));
        assert_eq!(response.body, b"body");
        assert!(response.is_success());
    }

    #[test]
    fn parses_session_header() {
        assert_eq!(parse_session_id("abc123;timeout=60").unwrap(), "abc123");
        assert!(parse_session_id("  ;timeout=60").is_err());
        assert_eq!(parse_session_timeout_secs("abc123;timeout=60"), Some(60));
        assert_eq!(parse_session_timeout_secs("abc123; Timeout = 30"), Some(30));
        assert_eq!(parse_session_timeout_secs("abc123"), None);
    }

    #[test]
    fn parses_interleaved_channel() {
        assert_eq!(
            parse_interleaved_channel(Some("RTP/AVP/TCP;unicast;interleaved=2-3")).unwrap(),
            2
        );
        assert!(matches!(
            parse_interleaved_channel(Some("RTP/AVP/TCP;unicast")),
            Err(RtspVideoSourceError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse_interleaved_channel(None),
            Err(RtspVideoSourceError::MissingHeader("Transport"))
        ));
    }
}
