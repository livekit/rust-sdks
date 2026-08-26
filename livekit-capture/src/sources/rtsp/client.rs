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

use super::{auth::RtspAuthContext, auth::RtspCredentials, RtspPhase, RtspVideoSourceError};

/// Socket read timeout: the poll granularity for the stop token, keepalives,
/// and deadlines, kept near one frame interval per the pump contract.
const READ_POLL: Duration = Duration::from_millis(100);

/// Socket write timeout; requests are small, so a stalled write means the
/// connection is gone.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Upper bound on an RTSP response header.
const MAX_HEADER_BYTES: usize = 64 * 1024;

/// Bytes requested from the socket per read.
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// A parsed `rtsp://` or `rtsps://` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RtspUrl {
    /// Request URI with any userinfo stripped, so credentials never appear
    /// on the wire outside the `Authorization` header.
    pub(super) request_uri: String,
    /// Value for the `Host` header, always including the port.
    pub(super) host_header: String,
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
        let (scheme, rest, tls) = if let Some(rest) = url.strip_prefix("rtsp://") {
            ("rtsp://", rest, false)
        } else if let Some(rest) = url.strip_prefix("rtsps://") {
            ("rtsps://", rest, true)
        } else {
            return Err(RtspVideoSourceError::InvalidUrl("expected rtsp:// or rtsps:// scheme"));
        };
        let (authority, path_suffix) = match rest.find('/') {
            Some(path_start) => (&rest[..path_start], &rest[path_start..]),
            None => (rest, ""),
        };

        let (credentials, host_port) = match authority.rsplit_once('@') {
            Some((userinfo, host_port)) => (Some(parse_userinfo(userinfo)?), host_port),
            None => (None, authority),
        };
        if host_port.is_empty() {
            return Err(RtspVideoSourceError::InvalidUrl("missing host"));
        }
        let default_port = if tls { 322 } else { 554 };
        let (connect_host, port) = parse_host_port(host_port, default_port)?;
        let host_header = if host_port.contains(':') {
            host_port.to_owned()
        } else {
            format!("{host_port}:{port}")
        };

        Ok(Self {
            request_uri: format!("{scheme}{host_port}{path_suffix}"),
            host_header,
            credentials,
            tls,
            connect_host,
            port,
        })
    }
}

fn parse_userinfo(userinfo: &str) -> Result<RtspCredentials, RtspVideoSourceError> {
    let (username, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    if username.is_empty() {
        return Err(RtspVideoSourceError::InvalidUrl("missing username"));
    }
    Ok(RtspCredentials {
        username: percent_decode(username),
        password: percent_decode(password),
    })
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

fn parse_host_port(
    host_port: &str,
    default_port: u16,
) -> Result<(String, u16), RtspVideoSourceError> {
    if let Some(rest) = host_port.strip_prefix('[') {
        let Some((host, after_host)) = rest.split_once(']') else {
            return Err(RtspVideoSourceError::InvalidUrl("malformed IPv6 host"));
        };
        let port = after_host.strip_prefix(':').map(parse_port).transpose()?.unwrap_or(default_port);
        return Ok((host.to_owned(), port));
    }

    if let Some((host, port)) = host_port.rsplit_once(':') {
        if !host.contains(':') {
            return Ok((host.to_owned(), parse_port(port)?));
        }
    }

    Ok((host_port.to_owned(), default_port))
}

fn parse_port(port: &str) -> Result<u16, RtspVideoSourceError> {
    port.parse().map_err(|_| RtspVideoSourceError::InvalidUrl("invalid port"))
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
    host_header: String,
    last_read_at: Instant,
}

// Manual so the read buffer's contents are not dumped; the authentication
// context redacts its own credentials.
impl fmt::Debug for RtspClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtspClient")
            .field("host_header", &self.host_header)
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
            host_header: url.host_header.clone(),
            last_read_at: Instant::now(),
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
        headers: &[(&str, &str)],
    ) -> Result<(), RtspVideoSourceError> {
        use fmt::Write as _;

        let cseq = self.cseq;
        self.cseq = self.cseq.saturating_add(1);
        let authorization = self.auth.header(method, uri)?;

        let mut request = String::with_capacity(256);
        // Writing to a `String` cannot fail.
        let _ = write!(request, "{method} {uri} RTSP/1.0\r\n");
        let _ = write!(request, "CSeq: {cseq}\r\n");
        let _ = write!(request, "User-Agent: livekit-capture/0.1\r\n");
        let _ = write!(request, "Host: {}\r\n", self.host_header);
        if let Some(authorization) = authorization {
            let _ = write!(request, "Authorization: {authorization}\r\n");
        }
        for (name, value) in headers {
            let _ = write!(request, "{name}: {value}\r\n");
        }
        request.push_str("\r\n");

        self.stream.write_all(request.as_bytes())?;
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
            if let Some((response, consumed)) = parse_response(&self.buf)? {
                self.buf.advance(consumed);
                return Ok(response);
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
        let Some(&magic) = self.buf.first() else {
            return Ok(None);
        };
        match magic {
            b'$' => {
                if self.buf.len() < 4 {
                    return Ok(None);
                }
                let channel = self.buf[1];
                let payload_len = u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize;
                if self.buf.len() < 4 + payload_len {
                    return Ok(None);
                }
                self.buf.advance(4);
                let payload = self.buf.split_to(payload_len).freeze();
                Ok(Some(InterleavedPoll::Frame { channel, payload }))
            }
            b'R' => match parse_response(&self.buf)? {
                Some((response, consumed)) => {
                    self.buf.advance(consumed);
                    Ok(Some(InterleavedPoll::Response(response)))
                }
                None => Ok(None),
            },
            _ => Err(RtspVideoSourceError::UnexpectedData),
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

/// Parses one RTSP response from the front of `buf`, returning the response
/// and the bytes it consumed, or `Ok(None)` when more bytes are needed.
fn parse_response(buf: &[u8]) -> Result<Option<(RtspResponse, usize)>, RtspVideoSourceError> {
    let Some(header_end) = find_header_end(buf) else {
        if buf.len() > MAX_HEADER_BYTES {
            return Err(RtspVideoSourceError::InvalidResponse("header too large"));
        }
        return Ok(None);
    };

    let header_text = str::from_utf8(&buf[..header_end])
        .map_err(|_| RtspVideoSourceError::InvalidResponse("header is not UTF-8"))?;
    let mut lines = header_text.split("\r\n");
    let status_line =
        lines.next().ok_or(RtspVideoSourceError::InvalidResponse("missing status line"))?;
    let mut status_parts = status_line.splitn(3, ' ');
    if status_parts.next() != Some("RTSP/1.0") {
        return Err(RtspVideoSourceError::InvalidResponse("unsupported version"));
    }
    let status_code = status_parts
        .next()
        .ok_or(RtspVideoSourceError::InvalidResponse("missing status code"))?
        .parse()
        .map_err(|_| RtspVideoSourceError::InvalidResponse("invalid status code"))?;
    let reason = status_parts.next().unwrap_or_default().to_owned();

    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(RtspVideoSourceError::InvalidResponse("malformed header"));
        };
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>())
        .transpose()
        .map_err(|_| RtspVideoSourceError::InvalidResponse("invalid content length"))?
        .unwrap_or(0);
    let body_start = header_end + 4;
    let Some(consumed) = body_start.checked_add(content_length) else {
        return Err(RtspVideoSourceError::InvalidResponse("invalid content length"));
    };
    if buf.len() < consumed {
        return Ok(None);
    }
    let body = buf[body_start..consumed].to_vec();

    Ok(Some((RtspResponse { status_code, reason, headers, body }, consumed)))
}

/// Finds the end of the response header (the start of `\r\n\r\n`).
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).take(MAX_HEADER_BYTES).position(|window| window == b"\r\n\r\n")
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
        assert_eq!(url.host_header, "camera.example:554");
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
        assert_eq!(url.host_header, "camera.example:322");
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
        assert_eq!(url.host_header, "camera.example:554");
        assert_eq!(url.request_uri, "rtsp://camera.example/live");
    }

    #[test]
    fn parses_bracketed_ipv6_host() {
        let url = RtspUrl::parse("rtsp://[2001:db8::1]:8554/live").unwrap();
        assert_eq!(url.connect_host, "2001:db8::1");
        assert_eq!(url.port, 8554);
        assert_eq!(url.host_header, "[2001:db8::1]:8554");
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
    fn parses_response_with_body_and_reports_consumed_bytes() {
        let bytes =
            b"RTSP/1.0 200 OK\r\nCSeq: 1\r\nContent-Length: 4\r\n\r\nbody$leftover";
        let (response, consumed) = parse_response(bytes).unwrap().unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.reason, "OK");
        assert_eq!(response.header("cseq"), Some("1"));
        assert_eq!(response.body, b"body");
        assert_eq!(&bytes[consumed..], b"$leftover");
    }

    #[test]
    fn incomplete_response_needs_more_bytes() {
        assert!(parse_response(b"RTSP/1.0 200 OK\r\nCSeq:").unwrap().is_none());
        assert!(parse_response(b"RTSP/1.0 200 OK\r\nContent-Length: 4\r\n\r\nbo")
            .unwrap()
            .is_none());
    }

    #[test]
    fn rejects_oversized_header() {
        let mut bytes = b"RTSP/1.0 200 OK\r\n".to_vec();
        bytes.resize(MAX_HEADER_BYTES + 8, b'a');
        assert!(matches!(
            parse_response(&bytes),
            Err(RtspVideoSourceError::InvalidResponse("header too large"))
        ));
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
