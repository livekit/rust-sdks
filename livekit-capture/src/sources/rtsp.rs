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

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    ops::Range,
    str,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose, Engine as _};
use md5::{Digest, Md5};
use thiserror::Error;

use crate::encoded::{
    ingress::EncodedAccessUnitSource,
    rtp::{RtpAccessUnitAssembler, RtpDepacketizerError},
    EncodedVideoCodec, OwnedEncodedAccessUnit,
};

const DEFAULT_RTSP_CLOCK_RATE: u32 = 90_000;
const MAX_RTSP_HEADER_BYTES: usize = 64 * 1024;
const RTSP_STREAM_READ_CHUNK_BYTES: usize = 8 * 1024;
const DEFAULT_RTSP_READ_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_RTSP_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Options used to open an RTSP encoded video source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtspSourceOptions {
    /// Expected video codec, when the caller wants to reject mismatched SDP.
    pub expected_codec: Option<EncodedVideoCodec>,
    /// Timestamp assigned to the first emitted access unit.
    pub start_timestamp_us: i64,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Non-zero socket read timeout applied to the RTSP TCP stream (default 10s).
    ///
    /// Handshake reads that exceed it fail with [`RtspSourceError::Timeout`].
    /// Streaming reads treat it as the retry granularity instead, so session
    /// keepalives keep flowing while the stream is silent.
    pub read_timeout: Duration,
    /// Maximum stream silence tolerated before [`RtspSourceError::Timeout`]
    /// (default 30s). Receiving any interleaved bytes resets the limit.
    pub idle_timeout: Duration,
}

impl RtspSourceOptions {
    /// Creates RTSP source options for encoded frames with the supplied dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            expected_codec: None,
            start_timestamp_us: 0,
            width,
            height,
            read_timeout: DEFAULT_RTSP_READ_TIMEOUT,
            idle_timeout: DEFAULT_RTSP_IDLE_TIMEOUT,
        }
    }

    /// Requires the SDP video track to use the supplied codec.
    pub fn with_expected_codec(mut self, codec: EncodedVideoCodec) -> Self {
        self.expected_codec = Some(codec);
        self
    }

    /// Sets the timestamp assigned to the first emitted access unit.
    pub fn with_start_timestamp_us(mut self, start_timestamp_us: i64) -> Self {
        self.start_timestamp_us = start_timestamp_us;
        self
    }

    /// Sets the socket read timeout.
    pub fn with_read_timeout(mut self, read_timeout: Duration) -> Self {
        self.read_timeout = read_timeout;
        self
    }

    /// Sets the maximum stream silence tolerated before a timeout error.
    pub fn with_idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.idle_timeout = idle_timeout;
        self
    }
}

/// RTSP session details discovered while opening a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtspSessionInfo {
    /// RTP payload codec selected from SDP.
    pub codec: EncodedVideoCodec,
    /// RTP payload type selected from SDP.
    pub payload_type: u8,
    /// RTP timestamp clock rate.
    pub clock_rate: u32,
    /// RTSP interleaved channel carrying video RTP packets.
    pub video_channel: u8,
    /// RTSP media control URL used for SETUP.
    pub control_url: String,
    /// RTSP session identifier returned by SETUP.
    pub session_id: String,
}

/// Encoded RTSP source that performs DESCRIBE, SETUP, and PLAY over TCP.
#[derive(Debug)]
pub struct RtspEncodedSource {
    source: RtspInterleavedRtpSource<TcpStream>,
    session_info: RtspSessionInfo,
    keepalive: RtspKeepalive,
}

impl RtspEncodedSource {
    /// Connects to an RTSP URL and starts TCP-interleaved RTP playback.
    pub fn connect(url: &str, options: RtspSourceOptions) -> Result<Self, RtspSourceError> {
        let rtsp_url = RtspUrl::parse(url)?;
        let mut stream = TcpStream::connect((rtsp_url.connect_host.as_str(), rtsp_url.port))
            .map_err(RtspSourceError::Io)?;
        let _ = stream.set_nodelay(true);
        stream.set_read_timeout(Some(options.read_timeout)).map_err(RtspSourceError::Io)?;
        let mut auth = RtspAuthContext::new(rtsp_url.credentials.clone());
        let mut cseq = 1;

        let describe = send_authenticated_rtsp_request(
            &mut stream,
            "DESCRIBE",
            &rtsp_url.original,
            &mut cseq,
            &[("Host", rtsp_url.host_header.as_str()), ("Accept", "application/sdp")],
            &mut auth,
        )?;
        let sdp = str::from_utf8(&describe.body).map_err(|_| RtspSourceError::InvalidSdp)?;
        let media = parse_sdp_video_track(&rtsp_url, sdp, options.expected_codec)?;

        let setup = send_authenticated_rtsp_request(
            &mut stream,
            "SETUP",
            &media.control_url,
            &mut cseq,
            &[
                ("Host", rtsp_url.host_header.as_str()),
                ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
            ],
            &mut auth,
        )?;
        let session_header =
            setup.header("session").ok_or(RtspSourceError::MissingHeader("Session"))?;
        let session_id = parse_session_id(session_header)?;
        let session_timeout_secs = parse_session_timeout_secs(session_header);
        let video_channel = parse_interleaved_channel(setup.header("transport"));

        send_authenticated_rtsp_request(
            &mut stream,
            "PLAY",
            &rtsp_url.original,
            &mut cseq,
            &[
                ("Host", rtsp_url.host_header.as_str()),
                ("Session", session_id.as_str()),
                ("Range", "npt=0.000-"),
            ],
            &mut auth,
        )?;

        let session_info = RtspSessionInfo {
            codec: media.codec,
            payload_type: media.payload_type,
            clock_rate: media.clock_rate,
            video_channel,
            control_url: media.control_url,
            session_id,
        };
        let config = RtspInterleavedSourceConfig {
            codec: session_info.codec,
            clock_rate: session_info.clock_rate,
            video_channel: session_info.video_channel,
            start_timestamp_us: options.start_timestamp_us,
            width: options.width,
            height: options.height,
            idle_timeout: options.idle_timeout,
        };
        let source = RtspInterleavedRtpSource::new(stream, config)?;
        let keepalive = RtspKeepalive::new(
            rtsp_url.original,
            rtsp_url.host_header,
            session_info.session_id.clone(),
            cseq,
            auth,
            session_timeout_secs,
        );

        Ok(Self { source, session_info, keepalive })
    }

    /// Returns RTSP session details discovered during setup.
    pub fn session_info(&self) -> &RtspSessionInfo {
        &self.session_info
    }

    /// Attempts to clone the underlying TCP stream.
    pub fn try_clone_stream(&self) -> io::Result<TcpStream> {
        self.source.reader().try_clone()
    }
}

impl EncodedAccessUnitSource for RtspEncodedSource {
    type Error = RtspSourceError;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        loop {
            self.keepalive.maybe_send(self.source.reader_mut())?;
            match self.source.poll_access_unit()? {
                AccessUnitPoll::AccessUnit(access_unit) => return Ok(Some(access_unit)),
                AccessUnitPoll::EndOfStream => return Ok(None),
                // A stream read timed out; loop so a due keepalive can be
                // sent even while the interleaved stream is silent.
                AccessUnitPoll::TimedOut => {}
            }
        }
    }
}

#[derive(Debug)]
struct RtspKeepalive {
    request_uri: String,
    host_header: String,
    session_id: String,
    cseq: u32,
    auth: RtspAuthContext,
    interval: Duration,
    next_due: Instant,
}

impl RtspKeepalive {
    fn new(
        request_uri: String,
        host_header: String,
        session_id: String,
        cseq: u32,
        auth: RtspAuthContext,
        session_timeout_secs: Option<u64>,
    ) -> Self {
        let interval_secs = session_timeout_secs.map(|timeout| (timeout / 2).max(1)).unwrap_or(30);
        let interval = Duration::from_secs(interval_secs);
        Self {
            request_uri,
            host_header,
            session_id,
            cseq,
            auth,
            interval,
            next_due: Instant::now() + interval,
        }
    }

    fn maybe_send(&mut self, stream: &mut TcpStream) -> Result<(), RtspSourceError> {
        if Instant::now() < self.next_due {
            return Ok(());
        }

        let authorization = self.auth.header("OPTIONS", &self.request_uri)?;
        write_rtsp_request(
            stream,
            "OPTIONS",
            &self.request_uri,
            next_cseq(&mut self.cseq),
            &[("Host", self.host_header.as_str()), ("Session", self.session_id.as_str())],
            authorization,
        )?;
        self.next_due = Instant::now() + self.interval;
        Ok(())
    }
}

/// Configuration for RTSP interleaved RTP media.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtspInterleavedSourceConfig {
    /// RTP payload codec.
    pub codec: EncodedVideoCodec,
    /// RTP timestamp clock rate.
    pub clock_rate: u32,
    /// RTSP interleaved channel carrying video RTP packets.
    pub video_channel: u8,
    /// Timestamp assigned to the first emitted access unit.
    pub start_timestamp_us: i64,
    /// Encoded frame width in pixels.
    pub width: u32,
    /// Encoded frame height in pixels.
    pub height: u32,
    /// Maximum stream silence tolerated before timed-out reads become a hard
    /// [`RtspSourceError::Timeout`]. Receiving any bytes resets the limit.
    pub idle_timeout: Duration,
}

/// Encoded source for RTSP interleaved RTP streams.
#[derive(Debug)]
pub struct RtspInterleavedRtpSource<R> {
    reader: R,
    config: RtspInterleavedSourceConfig,
    assembler: RtpAccessUnitAssembler,
    /// Unconsumed stream bytes; may end with a partial unit that is kept
    /// across timed-out reads so framing survives read timeouts.
    stream_buf: Vec<u8>,
    /// Consumed prefix of `stream_buf`, compacted before each fill.
    stream_pos: usize,
    /// When the last stream bytes were received, for the idle limit.
    last_read_at: Instant,
    eof: bool,
}

/// Progress from polling the interleaved stream for one access unit.
#[derive(Debug)]
enum AccessUnitPoll {
    /// A complete access unit was assembled.
    AccessUnit(OwnedEncodedAccessUnit),
    /// The stream ended cleanly at a unit boundary.
    EndOfStream,
    /// A read timed out mid-stream; retry after running periodic work.
    TimedOut,
}

/// Result of one attempt to read more interleaved stream bytes.
#[derive(Debug)]
enum StreamFill {
    Filled,
    Eof,
    TimedOut,
}

impl<R> RtspInterleavedRtpSource<R>
where
    R: Read,
{
    /// Creates a source for an RTSP stream that is already in interleaved RTP mode.
    pub fn new(reader: R, config: RtspInterleavedSourceConfig) -> Result<Self, RtspSourceError> {
        let assembler = RtpAccessUnitAssembler::new(
            config.codec,
            config.clock_rate,
            config.start_timestamp_us,
            config.width,
            config.height,
        )?;
        Ok(Self {
            reader,
            config,
            assembler,
            stream_buf: Vec::new(),
            stream_pos: 0,
            last_read_at: Instant::now(),
            eof: false,
        })
    }

    /// Returns the wrapped reader.
    pub fn reader(&self) -> &R {
        &self.reader
    }

    /// Returns the wrapped reader mutably.
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Consumes this source and returns its reader.
    pub fn into_reader(self) -> R {
        self.reader
    }

    /// Advances the stream until an access unit completes, the stream ends,
    /// or a read times out with framing state preserved for the next poll.
    fn poll_access_unit(&mut self) -> Result<AccessUnitPoll, RtspSourceError> {
        loop {
            if self.eof {
                return Ok(AccessUnitPoll::EndOfStream);
            }

            while let Some(unit) = parse_interleaved_unit(&self.stream_buf[self.stream_pos..])? {
                let unit_start = self.stream_pos;
                match unit {
                    ParsedInterleavedUnit::Frame { channel, payload, len } => {
                        self.stream_pos = unit_start + len;
                        if channel != self.config.video_channel {
                            continue;
                        }
                        let payload =
                            &self.stream_buf[unit_start + payload.start..unit_start + payload.end];
                        if let Some(access_unit) = self.assembler.push(payload)? {
                            return Ok(AccessUnitPoll::AccessUnit(access_unit));
                        }
                    }
                    ParsedInterleavedUnit::RtspResponse { len } => {
                        self.stream_pos = unit_start + len;
                    }
                }
            }

            match self.fill_stream_buf()? {
                StreamFill::Filled => {}
                StreamFill::Eof => {
                    self.eof = true;
                    return Ok(AccessUnitPoll::EndOfStream);
                }
                StreamFill::TimedOut => return Ok(AccessUnitPoll::TimedOut),
            }
        }
    }

    /// Reads more stream bytes into `stream_buf`, compacting consumed data first.
    fn fill_stream_buf(&mut self) -> Result<StreamFill, RtspSourceError> {
        if self.stream_pos > 0 {
            self.stream_buf.drain(..self.stream_pos);
            self.stream_pos = 0;
        }
        let filled = self.stream_buf.len();
        self.stream_buf.resize(filled + RTSP_STREAM_READ_CHUNK_BYTES, 0);
        loop {
            match self.reader.read(&mut self.stream_buf[filled..]) {
                Ok(0) => {
                    self.stream_buf.truncate(filled);
                    return if filled == 0 {
                        Ok(StreamFill::Eof)
                    } else {
                        // The stream ended inside an interleaved unit.
                        Err(RtspSourceError::Io(io::Error::from(io::ErrorKind::UnexpectedEof)))
                    };
                }
                Ok(read) => {
                    self.stream_buf.truncate(filled + read);
                    self.last_read_at = Instant::now();
                    return Ok(StreamFill::Filled);
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) if is_timeout_io_error(&err) => {
                    self.stream_buf.truncate(filled);
                    return if self.last_read_at.elapsed() >= self.config.idle_timeout {
                        Err(RtspSourceError::Timeout {
                            phase: "interleaved stream data".to_owned(),
                        })
                    } else {
                        Ok(StreamFill::TimedOut)
                    };
                }
                Err(err) => {
                    self.stream_buf.truncate(filled);
                    return Err(RtspSourceError::Io(err));
                }
            }
        }
    }
}

impl<R> EncodedAccessUnitSource for RtspInterleavedRtpSource<R>
where
    R: Read + Send + Sync + 'static,
{
    type Error = RtspSourceError;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        loop {
            match self.poll_access_unit()? {
                AccessUnitPoll::AccessUnit(access_unit) => return Ok(Some(access_unit)),
                AccessUnitPoll::EndOfStream => return Ok(None),
                // Keep waiting until the configured idle limit turns
                // timed-out reads into a hard error.
                AccessUnitPoll::TimedOut => {}
            }
        }
    }
}

/// One unit parsed from the front of the interleaved stream buffer.
#[derive(Debug)]
enum ParsedInterleavedUnit {
    /// Interleaved binary frame with its payload range and total length.
    Frame { channel: u8, payload: Range<usize>, len: usize },
    /// In-stream RTSP response (for example a keepalive reply) to skip.
    RtspResponse { len: usize },
}

/// Parses one interleaved unit from the front of `buf`, returning `Ok(None)`
/// when more bytes are needed.
fn parse_interleaved_unit(buf: &[u8]) -> Result<Option<ParsedInterleavedUnit>, RtspSourceError> {
    let Some(&magic) = buf.first() else {
        return Ok(None);
    };
    match magic {
        b'$' => {
            if buf.len() < 4 {
                return Ok(None);
            }
            let channel = buf[1];
            let len = 4 + u16::from_be_bytes([buf[2], buf[3]]) as usize;
            if buf.len() < len {
                return Ok(None);
            }
            Ok(Some(ParsedInterleavedUnit::Frame { channel, payload: 4..len, len }))
        }
        b'R' => {
            let mut remaining = buf;
            match read_rtsp_response(&mut remaining) {
                Ok(_response) => Ok(Some(ParsedInterleavedUnit::RtspResponse {
                    len: buf.len() - remaining.len(),
                })),
                Err(RtspSourceError::Io(err)) if err.kind() == io::ErrorKind::UnexpectedEof => {
                    Ok(None)
                }
                Err(err) => Err(err),
            }
        }
        _ => Err(RtspSourceError::UnexpectedData),
    }
}

/// Error returned by RTSP encoded sources.
#[derive(Debug, Error)]
pub enum RtspSourceError {
    /// I/O failed while reading RTSP interleaved data.
    #[error("RTSP I/O failed: {0}")]
    Io(io::Error),
    /// An RTSP read exceeded the configured timeout.
    #[error("RTSP timed out waiting for {phase}")]
    Timeout {
        /// Protocol phase or data the client was waiting for.
        phase: String,
    },
    /// RTSP URL was invalid or unsupported.
    #[error("invalid RTSP URL: {0}")]
    InvalidUrl(&'static str),
    /// RTSP server returned a non-success status.
    #[error("RTSP request failed with status {code} {reason}")]
    RtspStatus {
        /// RTSP status code.
        code: u16,
        /// RTSP status reason.
        reason: String,
    },
    /// RTSP response was malformed.
    #[error("invalid RTSP response: {0}")]
    InvalidResponse(&'static str),
    /// RTSP response was missing a required header.
    #[error("RTSP response missing {0} header")]
    MissingHeader(&'static str),
    /// RTSP server requested authentication but no URL credentials were supplied.
    #[error("RTSP authentication required but the URL does not contain credentials")]
    MissingCredentials,
    /// RTSP authentication challenge was malformed.
    #[error("invalid RTSP authentication challenge")]
    InvalidAuthChallenge,
    /// RTSP authentication scheme is not supported.
    #[error("unsupported RTSP authentication scheme: {0}")]
    UnsupportedAuthScheme(String),
    /// SDP was missing a supported video track.
    #[error("RTSP SDP does not contain a supported video track")]
    MissingVideoTrack,
    /// SDP did not offer the requested codec on any video track.
    #[error("RTSP SDP codec mismatch: expected {expected:?}, offered {actual:?}")]
    CodecMismatch {
        /// Codec requested by the caller.
        expected: EncodedVideoCodec,
        /// Supported codecs offered by the SDP video tracks.
        actual: Vec<EncodedVideoCodec>,
    },
    /// SDP body was malformed or not valid UTF-8.
    #[error("invalid RTSP SDP")]
    InvalidSdp,
    /// Interleaved RTP was malformed or a non-interleaved byte was encountered.
    #[error("unexpected RTSP interleaved data")]
    UnexpectedData,
    /// RTP depayloading failed.
    #[error(transparent)]
    Rtp(#[from] RtpDepacketizerError),
}

fn is_timeout_io_error(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RtspUrl {
    original: String,
    authority: String,
    connect_host: String,
    host_header: String,
    port: u16,
    credentials: Option<RtspCredentials>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RtspCredentials {
    username: String,
    password: String,
}

impl RtspUrl {
    fn parse(url: &str) -> Result<Self, RtspSourceError> {
        let Some(rest) = url.strip_prefix("rtsp://") else {
            return Err(RtspSourceError::InvalidUrl("expected rtsp:// scheme"));
        };
        let (authority, path_suffix) = match rest.find('/') {
            Some(path_start) => (&rest[..path_start], &rest[path_start..]),
            None => (rest, ""),
        };
        if authority.is_empty() {
            return Err(RtspSourceError::InvalidUrl("missing host"));
        }

        let (credentials, host_port) = match authority.rsplit_once('@') {
            Some((userinfo, host_port)) => (Some(parse_userinfo(userinfo)?), host_port),
            None => (None, authority),
        };
        if host_port.is_empty() {
            return Err(RtspSourceError::InvalidUrl("missing host"));
        }
        let (connect_host, port) = parse_host_port(host_port)?;
        let host_header = if host_port.contains(':') {
            host_port.to_owned()
        } else {
            format!("{host_port}:{port}")
        };

        Ok(Self {
            original: format!("rtsp://{host_port}{path_suffix}"),
            authority: host_port.to_owned(),
            connect_host,
            host_header,
            port,
            credentials,
        })
    }
}

fn parse_userinfo(userinfo: &str) -> Result<RtspCredentials, RtspSourceError> {
    let (username, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    if username.is_empty() {
        return Err(RtspSourceError::InvalidUrl("missing username"));
    }
    Ok(RtspCredentials { username: username.to_owned(), password: password.to_owned() })
}

fn parse_host_port(host_port: &str) -> Result<(String, u16), RtspSourceError> {
    if let Some(rest) = host_port.strip_prefix('[') {
        let Some((host, after_host)) = rest.split_once(']') else {
            return Err(RtspSourceError::InvalidUrl("malformed IPv6 host"));
        };
        let port = after_host.strip_prefix(':').map(parse_port).transpose()?.unwrap_or(554);
        return Ok((host.to_owned(), port));
    }

    if let Some((host, port)) = host_port.rsplit_once(':') {
        if !host.contains(':') {
            return Ok((host.to_owned(), parse_port(port)?));
        }
    }

    Ok((host_port.to_owned(), 554))
}

fn parse_port(port: &str) -> Result<u16, RtspSourceError> {
    port.parse().map_err(|_| RtspSourceError::InvalidUrl("invalid port"))
}

#[derive(Debug, Clone)]
struct RtspResponse {
    status_code: u16,
    reason: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl RtspResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn headers<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn send_authenticated_rtsp_request(
    stream: &mut TcpStream,
    method: &str,
    uri: &str,
    cseq: &mut u32,
    headers: &[(&str, &str)],
    auth: &mut RtspAuthContext,
) -> Result<RtspResponse, RtspSourceError> {
    let mut response = send_rtsp_request(
        stream,
        method,
        uri,
        next_cseq(cseq),
        headers,
        auth.header(method, uri)?,
    )?;
    if response.status_code == 401 {
        auth.update_from_unauthorized(&response)?;
        response = send_rtsp_request(
            stream,
            method,
            uri,
            next_cseq(cseq),
            headers,
            auth.header(method, uri)?,
        )?;
    }

    if !(200..300).contains(&response.status_code) {
        return Err(RtspSourceError::RtspStatus {
            code: response.status_code,
            reason: response.reason,
        });
    }
    Ok(response)
}

fn next_cseq(cseq: &mut u32) -> u32 {
    let current = *cseq;
    *cseq = cseq.saturating_add(1);
    current
}

fn send_rtsp_request(
    stream: &mut TcpStream,
    method: &str,
    uri: &str,
    cseq: u32,
    headers: &[(&str, &str)],
    authorization: Option<String>,
) -> Result<RtspResponse, RtspSourceError> {
    write_rtsp_request(stream, method, uri, cseq, headers, authorization)?;
    read_rtsp_response(stream).map_err(|err| match err {
        // Handshake reads must complete within the socket read timeout.
        RtspSourceError::Io(io_err) if is_timeout_io_error(&io_err) => {
            RtspSourceError::Timeout { phase: format!("{method} response") }
        }
        err => err,
    })
}

fn write_rtsp_request(
    stream: &mut TcpStream,
    method: &str,
    uri: &str,
    cseq: u32,
    headers: &[(&str, &str)],
    authorization: Option<String>,
) -> Result<(), RtspSourceError> {
    write!(stream, "{method} {uri} RTSP/1.0\r\n").map_err(RtspSourceError::Io)?;
    write!(stream, "CSeq: {cseq}\r\n").map_err(RtspSourceError::Io)?;
    write!(stream, "User-Agent: livekit-capture/0.1\r\n").map_err(RtspSourceError::Io)?;
    if let Some(authorization) = authorization {
        write!(stream, "Authorization: {authorization}\r\n").map_err(RtspSourceError::Io)?;
    }
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").map_err(RtspSourceError::Io)?;
    }
    write!(stream, "\r\n").map_err(RtspSourceError::Io)?;
    stream.flush().map_err(RtspSourceError::Io)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct RtspAuthContext {
    credentials: Option<RtspCredentials>,
    challenge: Option<RtspAuthChallenge>,
    nonce_count: u32,
    cnonce: String,
}

impl RtspAuthContext {
    fn new(credentials: Option<RtspCredentials>) -> Self {
        Self { credentials, challenge: None, nonce_count: 0, cnonce: make_cnonce() }
    }

    fn header(&mut self, method: &str, uri: &str) -> Result<Option<String>, RtspSourceError> {
        let Some(challenge) = self.challenge.clone() else {
            return Ok(None);
        };
        let credentials = self.credentials.as_ref().ok_or(RtspSourceError::MissingCredentials)?;
        match challenge {
            RtspAuthChallenge::Basic => {
                let token = general_purpose::STANDARD
                    .encode(format!("{}:{}", credentials.username, credentials.password));
                Ok(Some(format!("Basic {token}")))
            }
            RtspAuthChallenge::Digest(challenge) => {
                self.nonce_count = self.nonce_count.saturating_add(1);
                Ok(Some(build_digest_authorization(
                    credentials,
                    &challenge,
                    method,
                    uri,
                    self.nonce_count,
                    &self.cnonce,
                )))
            }
        }
    }

    fn update_from_unauthorized(&mut self, response: &RtspResponse) -> Result<(), RtspSourceError> {
        if self.credentials.is_none() {
            return Err(RtspSourceError::MissingCredentials);
        }
        self.challenge = Some(parse_authenticate_header(
            response.headers("www-authenticate").collect::<Vec<_>>().as_slice(),
        )?);
        self.nonce_count = 0;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RtspAuthChallenge {
    Basic,
    Digest(DigestAuthChallenge),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DigestAuthChallenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    qop: Option<String>,
}

fn parse_authenticate_header(headers: &[&str]) -> Result<RtspAuthChallenge, RtspSourceError> {
    for header in headers {
        if strip_auth_scheme(header, "Digest").is_some() {
            return parse_digest_challenge(header);
        }
    }
    for header in headers {
        if strip_auth_scheme(header, "Basic").is_some() {
            return Ok(RtspAuthChallenge::Basic);
        }
    }
    Err(RtspSourceError::UnsupportedAuthScheme(
        headers.first().copied().unwrap_or_default().to_owned(),
    ))
}

fn parse_digest_challenge(header: &str) -> Result<RtspAuthChallenge, RtspSourceError> {
    let params = parse_auth_params(
        strip_auth_scheme(header, "Digest").ok_or(RtspSourceError::InvalidAuthChallenge)?,
    );
    let realm = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("realm"))
        .map(|(_, value)| value.to_owned())
        .ok_or(RtspSourceError::InvalidAuthChallenge)?;
    let nonce = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("nonce"))
        .map(|(_, value)| value.to_owned())
        .ok_or(RtspSourceError::InvalidAuthChallenge)?;
    if let Some((_, algorithm)) =
        params.iter().find(|(name, _)| name.eq_ignore_ascii_case("algorithm"))
    {
        if !algorithm.eq_ignore_ascii_case("MD5") {
            return Err(RtspSourceError::UnsupportedAuthScheme(format!(
                "Digest algorithm={algorithm}"
            )));
        }
    }
    let qop = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("qop"))
        .and_then(|(_, value)| select_digest_qop(value));
    let opaque = params
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("opaque"))
        .map(|(_, value)| value.to_owned());

    Ok(RtspAuthChallenge::Digest(DigestAuthChallenge { realm, nonce, opaque, qop }))
}

fn strip_auth_scheme<'a>(header: &'a str, scheme: &str) -> Option<&'a str> {
    let header = header.trim_start();
    let rest = header.get(scheme.len()..)?;
    if !header[..scheme.len()].eq_ignore_ascii_case(scheme) {
        return None;
    }
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(' ')
}

fn parse_auth_params(params: &str) -> Vec<(String, String)> {
    let mut parsed = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in params.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => {
                escaped = true;
                current.push(ch);
            }
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                push_auth_param(&mut parsed, &current);
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    push_auth_param(&mut parsed, &current);
    parsed
}

fn push_auth_param(parsed: &mut Vec<(String, String)>, param: &str) {
    let Some((name, value)) = param.trim().split_once('=') else {
        return;
    };
    parsed.push((name.trim().to_owned(), unquote_auth_value(value.trim())));
}

fn unquote_auth_value(value: &str) -> String {
    let Some(value) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return value.to_owned();
    };
    let mut unquoted = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            unquoted.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            unquoted.push(ch);
        }
    }
    unquoted
}

fn select_digest_qop(value: &str) -> Option<String> {
    value.split(',').map(str::trim).find(|qop| qop.eq_ignore_ascii_case("auth")).map(str::to_owned)
}

fn build_digest_authorization(
    credentials: &RtspCredentials,
    challenge: &DigestAuthChallenge,
    method: &str,
    uri: &str,
    nonce_count: u32,
    cnonce: &str,
) -> String {
    let ha1 =
        md5_hex(format!("{}:{}:{}", credentials.username, challenge.realm, credentials.password));
    let ha2 = md5_hex(format!("{method}:{uri}"));
    let response = if let Some(qop) = &challenge.qop {
        md5_hex(format!("{ha1}:{}:{nonce_count:08x}:{cnonce}:{qop}:{ha2}", challenge.nonce))
    } else {
        md5_hex(format!("{ha1}:{}:{ha2}", challenge.nonce))
    };

    let mut header = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
        quote_auth_value(&credentials.username),
        quote_auth_value(&challenge.realm),
        quote_auth_value(&challenge.nonce),
        quote_auth_value(uri),
        response
    );
    if let Some(qop) = &challenge.qop {
        header.push_str(&format!(
            ", qop={}, nc={nonce_count:08x}, cnonce=\"{}\"",
            quote_auth_value(qop),
            quote_auth_value(cnonce)
        ));
    }
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(", opaque=\"{}\"", quote_auth_value(opaque)));
    }
    header
}

fn quote_auth_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn md5_hex(input: impl AsRef<[u8]>) -> String {
    format!("{:x}", Md5::digest(input))
}

fn make_cnonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{nanos:032x}")
}

fn read_rtsp_response(reader: &mut impl Read) -> Result<RtspResponse, RtspSourceError> {
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader.read_exact(&mut byte).map_err(RtspSourceError::Io)?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
        if header.len() > MAX_RTSP_HEADER_BYTES {
            return Err(RtspSourceError::InvalidResponse("header too large"));
        }
    }

    let header_text =
        str::from_utf8(&header).map_err(|_| RtspSourceError::InvalidResponse("header UTF-8"))?;
    let mut lines = header_text.trim_end_matches("\r\n\r\n").split("\r\n");
    let status_line =
        lines.next().ok_or(RtspSourceError::InvalidResponse("missing status line"))?;
    let mut status_parts = status_line.splitn(3, ' ');
    if status_parts.next() != Some("RTSP/1.0") {
        return Err(RtspSourceError::InvalidResponse("unsupported version"));
    }
    let status_code = status_parts
        .next()
        .ok_or(RtspSourceError::InvalidResponse("missing status code"))?
        .parse()
        .map_err(|_| RtspSourceError::InvalidResponse("invalid status code"))?;
    let reason = status_parts.next().unwrap_or_default().to_owned();

    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(RtspSourceError::InvalidResponse("malformed header"));
        };
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>())
        .transpose()
        .map_err(|_| RtspSourceError::InvalidResponse("invalid content length"))?
        .unwrap_or(0);
    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).map_err(RtspSourceError::Io)?;
    }

    Ok(RtspResponse { status_code, reason, headers, body })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SdpVideoTrack {
    codec: EncodedVideoCodec,
    payload_type: u8,
    clock_rate: u32,
    control_url: String,
}

#[derive(Debug, Clone, Default)]
struct PartialSdpVideoTrack {
    payload_types: Vec<u8>,
    rtp_maps: Vec<SdpRtpMap>,
    control: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SdpRtpMap {
    payload_type: u8,
    codec: EncodedVideoCodec,
    clock_rate: u32,
}

fn parse_sdp_video_track(
    base_url: &RtspUrl,
    sdp: &str,
    expected_codec: Option<EncodedVideoCodec>,
) -> Result<SdpVideoTrack, RtspSourceError> {
    let mut tracks = Vec::new();
    let mut current = None;

    for line in sdp.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(media) = line.strip_prefix("m=") {
            if let Some(track) = current.take() {
                tracks.push(track);
            }
            if let Some(video) = media.strip_prefix("video ") {
                current = Some(parse_video_media(video));
            }
            continue;
        }

        let Some(track) = current.as_mut() else {
            continue;
        };
        if let Some(control) = line.strip_prefix("a=control:") {
            track.control = Some(control.trim().to_owned());
        } else if let Some(rtpmap) = line.strip_prefix("a=rtpmap:") {
            if let Some(rtp_map) = parse_rtpmap(rtpmap) {
                track.rtp_maps.push(rtp_map);
            }
        }
    }
    if let Some(track) = current {
        tracks.push(track);
    }

    let mut offered = Vec::new();
    for track in tracks {
        for payload_type in &track.payload_types {
            let Some(rtp_map) = track.rtp_maps.iter().find(|map| map.payload_type == *payload_type)
            else {
                continue;
            };
            if let Some(expected) = expected_codec {
                if rtp_map.codec != expected {
                    if !offered.contains(&rtp_map.codec) {
                        offered.push(rtp_map.codec);
                    }
                    continue;
                }
            }

            return Ok(SdpVideoTrack {
                codec: rtp_map.codec,
                payload_type: *payload_type,
                clock_rate: rtp_map.clock_rate,
                control_url: resolve_control_url(base_url, track.control.as_deref()),
            });
        }
    }

    match expected_codec {
        Some(expected) if !offered.is_empty() => {
            Err(RtspSourceError::CodecMismatch { expected, actual: offered })
        }
        _ => Err(RtspSourceError::MissingVideoTrack),
    }
}

fn parse_video_media(media: &str) -> PartialSdpVideoTrack {
    let payload_types = media
        .split_whitespace()
        .skip(2)
        .filter_map(|payload_type| payload_type.parse().ok())
        .collect();
    PartialSdpVideoTrack { payload_types, ..Default::default() }
}

fn parse_rtpmap(rtpmap: &str) -> Option<SdpRtpMap> {
    let (payload_type, encoding) = rtpmap.trim().split_once(' ')?;
    let payload_type = payload_type.parse().ok()?;
    let mut encoding_parts = encoding.split('/');
    let codec_name = encoding_parts.next()?;
    let codec = parse_sdp_codec(codec_name)?;
    let clock_rate = encoding_parts
        .next()
        .and_then(|clock_rate| clock_rate.parse().ok())
        .unwrap_or(DEFAULT_RTSP_CLOCK_RATE);
    Some(SdpRtpMap { payload_type, codec, clock_rate })
}

fn parse_sdp_codec(codec_name: &str) -> Option<EncodedVideoCodec> {
    if codec_name.eq_ignore_ascii_case("H264") {
        Some(EncodedVideoCodec::H264)
    } else if codec_name.eq_ignore_ascii_case("H265") || codec_name.eq_ignore_ascii_case("HEVC") {
        Some(EncodedVideoCodec::H265)
    } else if codec_name.eq_ignore_ascii_case("VP8") {
        Some(EncodedVideoCodec::VP8)
    } else if codec_name.eq_ignore_ascii_case("VP9") {
        Some(EncodedVideoCodec::VP9)
    } else if codec_name.eq_ignore_ascii_case("AV1") {
        Some(EncodedVideoCodec::AV1)
    } else {
        None
    }
}

fn resolve_control_url(base_url: &RtspUrl, control: Option<&str>) -> String {
    let Some(control) = control.map(str::trim).filter(|control| !control.is_empty()) else {
        return base_url.original.clone();
    };
    if control == "*" {
        return base_url.original.clone();
    }
    if control.starts_with("rtsp://") {
        return control.to_owned();
    }
    if control.starts_with('/') {
        return format!("rtsp://{}{}", base_url.authority, control);
    }
    format!("{}/{}", base_url.original.trim_end_matches('/'), control)
}

fn parse_session_id(session_header: &str) -> Result<String, RtspSourceError> {
    let session_id = session_header.split(';').next().unwrap_or_default().trim();
    if session_id.is_empty() {
        return Err(RtspSourceError::InvalidResponse("empty session id"));
    }
    Ok(session_id.to_owned())
}

fn parse_session_timeout_secs(session_header: &str) -> Option<u64> {
    session_header.split(';').skip(1).find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        if name.trim().eq_ignore_ascii_case("timeout") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

fn parse_interleaved_channel(transport_header: Option<&str>) -> u8 {
    let Some(transport_header) = transport_header else {
        return 0;
    };
    for part in transport_header.split(';') {
        let Some(value) = part.trim().strip_prefix("interleaved=") else {
            continue;
        };
        if let Some(first) = value.split('-').next().and_then(|channel| channel.parse().ok()) {
            return first;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn rtp_packet(sequence_number: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Vec<u8> {
        let mut packet = Vec::with_capacity(12 + payload.len());
        packet.push(0x80);
        packet.push(if marker { 0x80 | 96 } else { 96 });
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(&timestamp.to_be_bytes());
        packet.extend_from_slice(&0x1122_3344_u32.to_be_bytes());
        packet.extend_from_slice(payload);
        packet
    }

    fn interleaved(channel: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.push(b'$');
        frame.push(channel);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn interleaved_config(video_channel: u8) -> RtspInterleavedSourceConfig {
        RtspInterleavedSourceConfig {
            codec: EncodedVideoCodec::H264,
            clock_rate: 90_000,
            video_channel,
            start_timestamp_us: 0,
            width: 640,
            height: 480,
            idle_timeout: Duration::from_secs(30),
        }
    }

    #[test]
    fn reads_rtsp_interleaved_rtp_access_unit() {
        let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
        let stream = interleaved(0, &packet);
        let mut source =
            RtspInterleavedRtpSource::new(Cursor::new(stream), interleaved_config(0)).unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        assert!(source.next_access_unit().unwrap().is_none());
    }

    #[test]
    fn skips_rtsp_response_between_interleaved_frames() {
        let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
        let mut stream = Vec::new();
        write_status_response(&mut stream, 4, &[], &[], 200, "OK");
        stream.extend_from_slice(&interleaved(0, &packet));
        let mut source =
            RtspInterleavedRtpSource::new(Cursor::new(stream), interleaved_config(0)).unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();

        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        assert!(source.next_access_unit().unwrap().is_none());
    }

    #[test]
    fn recovers_interleaved_framing_across_read_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            let frame = interleaved(0, &packet);
            // Split inside the 4-byte interleaved header and pause long
            // enough for several client read timeouts in between.
            let (head, tail) = frame.split_at(2);
            stream.write_all(head).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(150));
            stream.write_all(tail).unwrap();
            stream.flush().unwrap();
        });

        let client = std::net::TcpStream::connect(addr).unwrap();
        client.set_read_timeout(Some(Duration::from_millis(25))).unwrap();
        let mut source = RtspInterleavedRtpSource::new(client, interleaved_config(0)).unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();

        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        server.join().unwrap();
    }

    #[test]
    fn interleaved_stream_times_out_after_idle_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            // Stay silent past the client's idle limit before closing.
            thread::sleep(Duration::from_millis(500));
            drop(stream);
        });

        let client = std::net::TcpStream::connect(addr).unwrap();
        client.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
        let config = RtspInterleavedSourceConfig {
            idle_timeout: Duration::from_millis(80),
            ..interleaved_config(0)
        };
        let mut source = RtspInterleavedRtpSource::new(client, config).unwrap();

        let err = source.next_access_unit().unwrap_err();

        assert!(matches!(err, RtspSourceError::Timeout { .. }));
        server.join().unwrap();
    }

    #[test]
    fn parses_sdp_video_track() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let track = parse_sdp_video_track(&base_url, sdp, Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(track.codec, EncodedVideoCodec::H264);
        assert_eq!(track.payload_type, 96);
        assert_eq!(track.clock_rate, 90_000);
        assert_eq!(track.control_url, "rtsp://camera.example/live/trackID=1");
    }

    #[test]
    fn parses_vp8_vp9_and_av1_sdp_video_tracks() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();

        for (rtpmap, codec) in [
            ("VP8/90000", EncodedVideoCodec::VP8),
            ("VP9/90000", EncodedVideoCodec::VP9),
            ("AV1/90000", EncodedVideoCodec::AV1),
        ] {
            let sdp = format!(
                "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 {rtpmap}\r\n"
            );

            let track = parse_sdp_video_track(&base_url, &sdp, Some(codec)).unwrap();

            assert_eq!(track.codec, codec);
            assert_eq!(track.payload_type, 96);
            assert_eq!(track.clock_rate, 90_000);
        }
    }

    #[test]
    fn rejects_sdp_codec_mismatch_for_vpx_av1() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 VP9/90000\r\n";

        let err = parse_sdp_video_track(&base_url, sdp, Some(EncodedVideoCodec::AV1)).unwrap_err();

        match err {
            RtspSourceError::CodecMismatch { expected, actual } => {
                assert_eq!(expected, EncodedVideoCodec::AV1);
                assert_eq!(actual, vec![EncodedVideoCodec::VP9]);
            }
            other => panic!("expected codec mismatch, got {other:?}"),
        }
    }

    #[test]
    fn selects_expected_codec_among_multiple_payload_types() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
a=rtpmap:96 H264/90000\r\n";

        let track = parse_sdp_video_track(&base_url, sdp, Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(track.codec, EncodedVideoCodec::H264);
        assert_eq!(track.payload_type, 96);
        assert_eq!(track.clock_rate, 90_000);
        assert_eq!(track.control_url, "rtsp://camera.example/live/trackID=1");
    }

    #[test]
    fn selects_expected_codec_from_later_video_section() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=2\r\n\
a=rtpmap:96 H264/90000\r\n";

        let track = parse_sdp_video_track(&base_url, sdp, Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(track.codec, EncodedVideoCodec::H264);
        assert_eq!(track.payload_type, 96);
        assert_eq!(track.control_url, "rtsp://camera.example/live/trackID=2");
    }

    #[test]
    fn rejects_sdp_listing_all_offered_codecs_when_none_match() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
a=rtpmap:96 H264/90000\r\n";

        let err = parse_sdp_video_track(&base_url, sdp, Some(EncodedVideoCodec::VP8)).unwrap_err();

        match err {
            RtspSourceError::CodecMismatch { expected, actual } => {
                assert_eq!(expected, EncodedVideoCodec::VP8);
                assert_eq!(actual, vec![EncodedVideoCodec::H265, EncodedVideoCodec::H264]);
            }
            other => panic!("expected codec mismatch, got {other:?}"),
        }
    }

    #[test]
    fn resolves_absolute_path_control_url() {
        let base_url = RtspUrl::parse("rtsp://camera.example/live").unwrap();
        assert_eq!(
            resolve_control_url(&base_url, Some("/stream/trackID=1")),
            "rtsp://camera.example/stream/trackID=1"
        );
    }

    #[test]
    fn parses_session_timeout() {
        assert_eq!(parse_session_timeout_secs("abc123;timeout=60"), Some(60));
        assert_eq!(parse_session_timeout_secs("abc123; Timeout = 30"), Some(30));
        assert_eq!(parse_session_timeout_secs("abc123"), None);
    }

    #[test]
    fn parses_credentials_but_strips_them_from_request_url() {
        let url = RtspUrl::parse("rtsp://admin:secret@camera.example:554/live").unwrap();

        assert_eq!(url.original, "rtsp://camera.example:554/live");
        assert_eq!(url.authority, "camera.example:554");
        assert_eq!(
            url.credentials,
            Some(RtspCredentials { username: "admin".to_owned(), password: "secret".to_owned() })
        );
    }

    #[test]
    fn builds_digest_authorization_with_qop_auth() {
        let credentials = RtspCredentials {
            username: "Mufasa".to_owned(),
            password: "Circle Of Life".to_owned(),
        };
        let challenge = DigestAuthChallenge {
            realm: "testrealm@host.com".to_owned(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".to_owned(),
            opaque: Some("5ccc069c403ebaf9f0171e9517f40e41".to_owned()),
            qop: Some("auth".to_owned()),
        };

        let authorization = build_digest_authorization(
            &credentials,
            &challenge,
            "GET",
            "/dir/index.html",
            1,
            "0a4f113b",
        );

        assert!(authorization.contains("response=\"6629fae49393a05397450978507c4ef1\""));
        assert!(authorization.contains("qop=auth"));
        assert!(authorization.contains("nc=00000001"));
    }

    #[test]
    fn sends_rtsp_keepalive_when_due() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream)
        });
        let mut client = std::net::TcpStream::connect(addr).unwrap();
        let mut keepalive = RtspKeepalive::new(
            "rtsp://camera.example/live".to_owned(),
            "camera.example:554".to_owned(),
            "abc123".to_owned(),
            4,
            RtspAuthContext::new(None),
            Some(2),
        );
        keepalive.next_due = Instant::now() - Duration::from_secs(1);

        keepalive.maybe_send(&mut client).unwrap();
        let request = server.join().unwrap();

        assert!(request.starts_with("OPTIONS rtsp://camera.example/live RTSP/1.0"));
        assert!(request.contains("CSeq: 4"));
        assert!(request.contains("Session: abc123"));
    }

    #[test]
    fn connects_and_reads_rtsp_access_unit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let describe = read_request(&mut stream);
            assert!(describe.starts_with("DESCRIBE rtsp://"));
            let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 H264/90000\r\n";
            write_response(
                &mut stream,
                1,
                &[("Content-Type", "application/sdp"), ("Content-Length", &sdp.len().to_string())],
                sdp.as_bytes(),
            );

            let setup = read_request(&mut stream);
            assert!(setup.starts_with("SETUP rtsp://"));
            assert!(setup.contains("Transport: RTP/AVP/TCP;unicast;interleaved=0-1"));
            write_response(
                &mut stream,
                2,
                &[
                    ("Session", "abc123;timeout=60"),
                    ("Transport", "RTP/AVP/TCP;unicast;interleaved=2-3"),
                ],
                &[],
            );

            let play = read_request(&mut stream);
            assert!(play.starts_with("PLAY rtsp://"));
            assert!(play.contains("Session: abc123"));
            write_response(&mut stream, 3, &[], &[]);

            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(2, &packet)).unwrap();
        });

        let options = RtspSourceOptions::new(640, 480)
            .with_expected_codec(EncodedVideoCodec::H264)
            .with_start_timestamp_us(0);
        let mut source =
            RtspEncodedSource::connect(&format!("rtsp://{addr}/camera"), options).unwrap();
        assert_eq!(source.session_info().codec, EncodedVideoCodec::H264);
        assert_eq!(source.session_info().video_channel, 2);
        assert_eq!(source.session_info().session_id, "abc123");

        let access_unit = source.next_access_unit().unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        server.join().unwrap();
    }

    #[test]
    fn connects_with_rtsp_digest_auth() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let first_describe = read_request(&mut stream);
            assert!(first_describe.starts_with(&format!("DESCRIBE rtsp://{addr}/camera")));
            assert!(!first_describe.contains("Authorization:"));
            write_status_response(
                &mut stream,
                1,
                &[("WWW-Authenticate", "Digest realm=\"camera\", nonce=\"abcdef\", qop=\"auth\"")],
                &[],
                401,
                "Unauthorized",
            );

            let second_describe = read_request(&mut stream);
            assert!(second_describe.starts_with(&format!("DESCRIBE rtsp://{addr}/camera")));
            assert!(!second_describe.contains("admin:secret@"));
            assert!(second_describe.contains("Authorization: Digest username=\"admin\""));
            assert!(second_describe.contains(&format!("uri=\"rtsp://{addr}/camera\"")));
            assert!(second_describe.contains("qop=auth"));
            let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 H264/90000\r\n";
            write_status_response(
                &mut stream,
                2,
                &[("Content-Type", "application/sdp"), ("Content-Length", &sdp.len().to_string())],
                sdp.as_bytes(),
                200,
                "OK",
            );

            let setup = read_request(&mut stream);
            assert!(setup.contains("Authorization: Digest username=\"admin\""));
            write_status_response(
                &mut stream,
                3,
                &[
                    ("Session", "abc123;timeout=60"),
                    ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
                ],
                &[],
                200,
                "OK",
            );

            let play = read_request(&mut stream);
            assert!(play.contains("Authorization: Digest username=\"admin\""));
            write_status_response(&mut stream, 4, &[], &[], 200, "OK");

            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(0, &packet)).unwrap();
        });

        let options = RtspSourceOptions::new(640, 480)
            .with_expected_codec(EncodedVideoCodec::H264)
            .with_start_timestamp_us(0);
        let mut source =
            RtspEncodedSource::connect(&format!("rtsp://admin:secret@{addr}/camera"), options)
                .unwrap();

        let access_unit = source.next_access_unit().unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        server.join().unwrap();
    }

    #[test]
    fn sends_keepalive_during_stream_silence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _describe = read_request(&mut stream);
            let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 H264/90000\r\n";
            write_response(
                &mut stream,
                1,
                &[("Content-Type", "application/sdp"), ("Content-Length", &sdp.len().to_string())],
                sdp.as_bytes(),
            );
            let _setup = read_request(&mut stream);
            write_response(
                &mut stream,
                2,
                &[
                    ("Session", "abc123;timeout=60"),
                    ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
                ],
                &[],
            );
            let _play = read_request(&mut stream);
            write_response(&mut stream, 3, &[], &[]);

            // Send no interleaved data; the keepalive must arrive during the
            // silence. Only then reply and send the first video frame.
            let keepalive = read_request(&mut stream);
            write_response(&mut stream, 4, &[], &[]);
            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(0, &packet)).unwrap();
            keepalive
        });

        let options = RtspSourceOptions::new(640, 480)
            .with_expected_codec(EncodedVideoCodec::H264)
            .with_read_timeout(Duration::from_millis(100))
            .with_idle_timeout(Duration::from_secs(5));
        let mut source =
            RtspEncodedSource::connect(&format!("rtsp://{addr}/camera"), options).unwrap();
        source.keepalive.next_due = Instant::now() + Duration::from_millis(250);

        let access_unit = source.next_access_unit().unwrap().unwrap();

        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        let keepalive = server.join().unwrap();
        assert!(keepalive.starts_with("OPTIONS rtsp://"));
        assert!(keepalive.contains("Session: abc123"));
    }

    #[test]
    fn handshake_read_timeout_is_hard_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _describe = read_request(&mut stream);
            // Never respond; hold the connection open past the read timeout.
            thread::sleep(Duration::from_millis(300));
        });

        let options = RtspSourceOptions::new(640, 480).with_read_timeout(Duration::from_millis(50));
        let err =
            RtspEncodedSource::connect(&format!("rtsp://{addr}/camera"), options).unwrap_err();

        assert!(
            matches!(&err, RtspSourceError::Timeout { phase } if phase.contains("DESCRIBE")),
            "expected DESCRIBE timeout, got {err:?}"
        );
        server.join().unwrap();
    }

    fn read_request(stream: &mut impl Read) -> String {
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).unwrap()
    }

    fn write_response(stream: &mut impl Write, cseq: u32, headers: &[(&str, &str)], body: &[u8]) {
        write_status_response(stream, cseq, headers, body, 200, "OK");
    }

    fn write_status_response(
        stream: &mut impl Write,
        cseq: u32,
        headers: &[(&str, &str)],
        body: &[u8],
        status_code: u16,
        reason: &str,
    ) {
        write!(stream, "RTSP/1.0 {status_code} {reason}\r\nCSeq: {cseq}\r\n").unwrap();
        for (name, value) in headers {
            write!(stream, "{name}: {value}\r\n").unwrap();
        }
        write!(stream, "\r\n").unwrap();
        if !body.is_empty() {
            stream.write_all(body).unwrap();
        }
        stream.flush().unwrap();
    }
}
