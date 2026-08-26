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

//! Encoded video capture from an RTSP server.
//!
//! [`RtspVideoSource`] connects to an `rtsp://` URL, negotiates a video
//! stream over TCP-interleaved RTP, and yields its access units without
//! re-encoding. Basic and Digest authentication are supported, and packet
//! loss is recovered by waiting for the next keyframe.
//!
//! The connection is not re-established on failure: a connection error ends
//! the source with an error, and a clean server-side end of stream ends it
//! like any finite source.

mod auth;
mod bits;
mod client;
mod dimensions;
mod rtp;
mod sdp;

use std::{
    fmt, io, str,
    time::{Duration, Instant},
};

use thiserror::Error;

use crate::{
    encoded::{EncodedFrameType, EncodedVideoCodec, EncodedVideoSource, OwnedEncodedAccessUnit},
    error::SourceError,
    primitive::VideoResolution,
    pump::PumpStop,
};
use auth::RtspCredentials;
use client::{
    parse_interleaved_channel, parse_session_id, parse_session_timeout_secs, InterleavedPoll,
    RtspClient, RtspUrl,
};
use rtp::RtpAccessUnitAssembler;

/// Default TCP connect and handshake timeout.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default maximum stream silence tolerated before the source fails.
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long resolution discovery waits for the stream's first keyframe.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Session timeout assumed when the server's `Session` header declares none
/// (RFC 2326 section 12.37).
const DEFAULT_SESSION_TIMEOUT_SECS: u64 = 60;

/// Configuration for an RTSP encoded video source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RtspVideoSourceConfig {
    /// RTSP URL (`rtsp://host[:port]/path`).
    ///
    /// URL userinfo (`rtsp://user:password@...`) is accepted and stripped
    /// from requests; [`Self::username`] and [`Self::password`] take
    /// precedence over it.
    pub url: String,

    /// Username for RTSP authentication, overriding URL userinfo.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub username: Option<String>,

    /// Password for RTSP authentication, overriding URL userinfo.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub password: Option<String>,

    /// Codec required from the stream. When omitted, the first supported
    /// video track offered by the SDP is used.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub codec: Option<EncodedVideoCodec>,

    /// Encoded frame resolution.
    ///
    /// When omitted, the resolution is discovered from the SDP when it
    /// declares one, and from the stream's first keyframe otherwise, so
    /// construction may wait for the stream to produce data. When set,
    /// construction returns without waiting, and the first keyframe is
    /// verified against this value.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub resolution: Option<VideoResolution>,

    /// TCP connect and RTSP handshake timeout in milliseconds
    /// (default 10000).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub connect_timeout_ms: Option<u32>,

    /// Maximum stream silence tolerated before the source fails, in
    /// milliseconds (default 30000). Receiving any stream bytes resets the
    /// limit.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub idle_timeout_ms: Option<u32>,
}

/// Protocol phase a timeout occurred in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RtspPhase {
    /// Establishing the TCP connection.
    Connect,
    /// Waiting for the DESCRIBE response.
    Describe,
    /// Waiting for the SETUP response.
    Setup,
    /// Waiting for the PLAY response.
    Play,
    /// Waiting for interleaved stream data.
    Stream,
}

impl fmt::Display for RtspPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Connect => "the TCP connection",
            Self::Describe => "the DESCRIBE response",
            Self::Setup => "the SETUP response",
            Self::Play => "the PLAY response",
            Self::Stream => "stream data",
        })
    }
}

/// Error returned by RTSP encoded video sources.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RtspVideoSourceError {
    /// I/O on the RTSP connection failed.
    #[error("RTSP I/O failed: {0}")]
    Io(#[from] io::Error),
    /// A protocol phase exceeded its timeout.
    #[error("RTSP timed out waiting for {phase}")]
    Timeout {
        /// Protocol phase the client was waiting on.
        phase: RtspPhase,
    },
    /// The RTSP URL was invalid or unsupported.
    #[error("invalid RTSP URL: {0}")]
    InvalidUrl(&'static str),
    /// The server returned a non-success status.
    #[error("RTSP request failed with status {code} {reason}")]
    RtspStatus {
        /// RTSP status code.
        code: u16,
        /// RTSP status reason.
        reason: String,
    },
    /// A response was malformed.
    #[error("invalid RTSP response: {0}")]
    InvalidResponse(&'static str),
    /// A response was missing a required header.
    #[error("RTSP response missing {0} header")]
    MissingHeader(&'static str),
    /// The server requires authentication but no credentials were supplied.
    #[error("RTSP authentication required but no credentials were supplied")]
    MissingCredentials,
    /// The authentication challenge was malformed.
    #[error("invalid RTSP authentication challenge")]
    InvalidAuthChallenge,
    /// The authentication scheme is not supported.
    #[error("unsupported RTSP authentication scheme: {0}")]
    UnsupportedAuthScheme(String),
    /// The Digest algorithm is not supported.
    #[error("unsupported RTSP Digest algorithm: {0}")]
    UnsupportedDigestAlgorithm(String),
    /// The SDP was missing a supported video track.
    #[error("RTSP SDP does not contain a supported video track")]
    MissingVideoTrack,
    /// The SDP did not offer the requested codec on any video track.
    #[error("RTSP SDP codec mismatch: expected {expected:?}, offered {offered:?}")]
    CodecMismatch {
        /// Codec required by the configuration.
        expected: EncodedVideoCodec,
        /// Supported codecs the SDP video tracks offered instead.
        offered: Vec<EncodedVideoCodec>,
    },
    /// The SDP body was malformed or not valid UTF-8.
    #[error("invalid RTSP SDP")]
    InvalidSdp,
    /// Interleaved framing was malformed or a non-interleaved byte arrived.
    #[error("unexpected RTSP interleaved data")]
    UnexpectedData,
    /// The stream produced no keyframe during resolution discovery.
    #[error(
        "stream produced no keyframe during resolution discovery; declare `resolution` in the \
         configuration to skip discovery"
    )]
    DiscoveryTimeout,
    /// The first keyframe did not carry parseable dimensions.
    #[error(
        "could not determine the stream resolution from its first keyframe; declare \
         `resolution` in the configuration"
    )]
    DiscoveryFailed,
    /// The stream ended before resolution discovery completed.
    #[error("stream ended before resolution discovery completed")]
    EndedDuringDiscovery,
    /// The stream's resolution does not match the established one.
    #[error("stream produces {actual}, but the source was configured for {configured}")]
    ResolutionMismatch {
        /// Resolution the source was configured for or discovered.
        configured: VideoResolution,
        /// Resolution the stream's keyframe declares.
        actual: VideoResolution,
    },
    /// RTP depacketization failed.
    #[error("invalid RTP stream: {0}")]
    Rtp(Box<dyn std::error::Error + Send + Sync>),
}

impl From<rtp::RtpDepacketizerError> for RtspVideoSourceError {
    fn from(err: rtp::RtpDepacketizerError) -> Self {
        Self::Rtp(Box::new(err))
    }
}

/// Progress from one bounded poll of the stream.
enum Poll {
    AccessUnit(OwnedEncodedAccessUnit),
    TimedOut,
    EndOfStream,
}

/// Encoded source that plays a video stream from an RTSP server.
pub struct RtspVideoSource {
    client: RtspClient,
    assembler: RtpAccessUnitAssembler,
    session_id: String,
    aggregate_control_url: String,
    rtp_channel: u8,
    keepalive_interval: Duration,
    keepalive_due: Instant,
    idle_timeout: Duration,
    codec: EncodedVideoCodec,
    resolution: VideoResolution,
    resolution_verified: bool,
    // Access unit pulled during resolution discovery, handed out first.
    pending: Option<OwnedEncodedAccessUnit>,
    eof: bool,
}

impl fmt::Debug for RtspVideoSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtspVideoSource")
            .field("codec", &self.codec)
            .field("resolution", &self.resolution)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl RtspVideoSource {
    /// Creates the source. Connection, handshake, and resolution discovery
    /// run on the tokio blocking pool.
    ///
    /// Requires a running tokio runtime. Use
    /// [`RtspVideoSource::new_blocking`] outside of async contexts.
    #[cfg(feature = "tokio")]
    pub async fn new(config: RtspVideoSourceConfig) -> Result<Self, SourceError> {
        crate::utils::run_blocking(move || Self::new_blocking(config)).await
    }

    /// Connects to the RTSP server and starts playback.
    ///
    /// The handshake (DESCRIBE, SETUP, PLAY over TCP-interleaved transport)
    /// is bounded by the connect timeout. Construction fails on an invalid
    /// URL, a missing or codec-mismatched video track, an authentication
    /// failure, or any non-success response.
    ///
    /// When the configuration declares no resolution and the SDP none
    /// either, this blocks until the stream's first keyframe arrives
    /// (bounded by a timeout) to read the dimensions from it.
    pub fn new_blocking(config: RtspVideoSourceConfig) -> Result<Self, SourceError> {
        Self::connect(config).map_err(SourceError::new)
    }

    fn connect(config: RtspVideoSourceConfig) -> Result<Self, RtspVideoSourceError> {
        let url = RtspUrl::parse(&config.url)?;
        let credentials = merge_credentials(&config, &url);
        let connect_timeout = duration_ms(config.connect_timeout_ms, DEFAULT_CONNECT_TIMEOUT);
        let idle_timeout = duration_ms(config.idle_timeout_ms, DEFAULT_IDLE_TIMEOUT);
        let handshake_deadline = Instant::now() + connect_timeout;

        let mut client = RtspClient::connect(&url, credentials, handshake_deadline)?;

        let describe = client.request(
            "DESCRIBE",
            &url.request_uri,
            &[("Accept", "application/sdp")],
            handshake_deadline,
            RtspPhase::Describe,
        )?;
        let sdp_text =
            str::from_utf8(&describe.body).map_err(|_| RtspVideoSourceError::InvalidSdp)?;
        // Relative control URLs resolve against the Content-Base per
        // RFC 2326 appendix C.1.1, falling back to the request URL.
        let base_url = describe
            .header("content-base")
            .or_else(|| describe.header("content-location"))
            .unwrap_or(&url.request_uri);
        let session = sdp::parse_sdp_session(base_url, sdp_text, config.codec)?;

        let setup = client.request(
            "SETUP",
            &session.video.control_url,
            &[("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1")],
            handshake_deadline,
            RtspPhase::Setup,
        )?;
        let session_header =
            setup.header("session").ok_or(RtspVideoSourceError::MissingHeader("Session"))?;
        let session_id = parse_session_id(session_header)?;
        let session_timeout_secs =
            parse_session_timeout_secs(session_header).unwrap_or(DEFAULT_SESSION_TIMEOUT_SECS);
        let rtp_channel = parse_interleaved_channel(setup.header("transport"))?;

        client.request(
            "PLAY",
            &session.aggregate_control_url,
            &[("Session", session_id.as_str()), ("Range", "npt=0.000-")],
            handshake_deadline,
            RtspPhase::Play,
        )?;

        // Resolution declared in the configuration wins; otherwise use the
        // SDP's hints (`a=framesize`, an out-of-band SPS), which spare
        // waiting for media; otherwise discover from the first keyframe.
        let codec = session.video.codec;
        let sdp_resolution = config.resolution.or(session.video.framesize).or_else(|| {
            session
                .video
                .parameter_sets
                .sps
                .iter()
                .find_map(|sps| dimensions::sps_resolution(codec, sps))
        });

        let assembler = RtpAccessUnitAssembler::new(
            codec,
            session.video.payload_type,
            session.video.clock_rate,
            session.video.parameter_sets.clone(),
            sdp_resolution.unwrap_or_default(),
        )?;

        let keepalive_interval = Duration::from_secs((session_timeout_secs / 2).max(1));
        let mut source = Self {
            client,
            assembler,
            session_id,
            aggregate_control_url: session.aggregate_control_url,
            rtp_channel,
            keepalive_interval,
            keepalive_due: Instant::now() + keepalive_interval,
            idle_timeout,
            codec,
            resolution: sdp_resolution.unwrap_or_default(),
            resolution_verified: false,
            pending: None,
            eof: false,
        };

        // From here on the session is live, so any failure path runs the
        // source's TEARDOWN through `Drop`.
        if sdp_resolution.is_none() {
            source.discover_resolution()?;
        }

        log::info!(
            "RTSP stream ready: {:?} {} ({} resolution)",
            source.codec,
            source.resolution,
            if config.resolution.is_some() { "declared" } else { "discovered" },
        );
        Ok(source)
    }

    /// Blocks until the stream's first keyframe reveals the resolution; the
    /// keyframe is kept so it is not lost to discovery.
    fn discover_resolution(&mut self) -> Result<(), RtspVideoSourceError> {
        let deadline = Instant::now() + DISCOVERY_TIMEOUT;
        loop {
            match self.poll_access_unit()? {
                Poll::AccessUnit(mut access_unit) => {
                    if access_unit.frame_type != EncodedFrameType::Key {
                        // Undecodable without the keyframe; the pump would
                        // drop these pre-roll deltas anyway.
                        continue;
                    }
                    let resolution =
                        dimensions::access_unit_resolution(self.codec, &access_unit.payload)
                            .ok_or(RtspVideoSourceError::DiscoveryFailed)?;
                    access_unit.resolution = resolution;
                    self.resolution = resolution;
                    self.assembler.set_resolution(resolution);
                    // The dimensions came from this keyframe itself.
                    self.resolution_verified = true;
                    self.pending = Some(access_unit);
                    return Ok(());
                }
                Poll::TimedOut => {
                    if Instant::now() >= deadline {
                        return Err(RtspVideoSourceError::DiscoveryTimeout);
                    }
                }
                Poll::EndOfStream => return Err(RtspVideoSourceError::EndedDuringDiscovery),
            }
        }
    }

    /// Advances the stream by at most one bounded read, running the
    /// keepalive and idle-limit bookkeeping.
    fn poll_access_unit(&mut self) -> Result<Poll, RtspVideoSourceError> {
        loop {
            if let Some(access_unit) = self.assembler.pop_ready() {
                return Ok(Poll::AccessUnit(access_unit));
            }
            self.maybe_send_keepalive()?;

            match self.client.poll_unit()? {
                InterleavedPoll::Frame { channel, payload } if channel == self.rtp_channel => {
                    self.assembler.push(&payload)?;
                }
                // Other channels carry RTCP and non-selected media.
                InterleavedPoll::Frame { .. } => {}
                InterleavedPoll::Response(response) => {
                    // In-band responses answer keepalives; a failure (for
                    // example 454 Session Not Found) means the session died.
                    if !response.is_success() {
                        return Err(RtspVideoSourceError::RtspStatus {
                            code: response.status_code,
                            reason: response.reason,
                        });
                    }
                }
                InterleavedPoll::TimedOut => {
                    if self.client.idle_for() >= self.idle_timeout {
                        return Err(RtspVideoSourceError::Timeout { phase: RtspPhase::Stream });
                    }
                    return Ok(Poll::TimedOut);
                }
                InterleavedPoll::EndOfStream => return Ok(Poll::EndOfStream),
            }
        }
    }

    /// Sends an OPTIONS keepalive when one is due, so the server keeps the
    /// session alive across stream silence. The reply arrives in-band.
    fn maybe_send_keepalive(&mut self) -> Result<(), RtspVideoSourceError> {
        if Instant::now() < self.keepalive_due {
            return Ok(());
        }
        self.client.write_request(
            "OPTIONS",
            &self.aggregate_control_url,
            &[("Session", self.session_id.as_str())],
        )?;
        self.keepalive_due = Instant::now() + self.keepalive_interval;
        Ok(())
    }

    /// Verifies the established resolution against the first keyframe that
    /// carries parseable dimensions.
    fn verify_resolution(
        &mut self,
        access_unit: &OwnedEncodedAccessUnit,
    ) -> Result<(), RtspVideoSourceError> {
        if self.resolution_verified || access_unit.frame_type != EncodedFrameType::Key {
            return Ok(());
        }
        self.resolution_verified = true;
        let Some(actual) = dimensions::access_unit_resolution(self.codec, &access_unit.payload)
        else {
            log::debug!("RTSP keyframe carries no parseable dimensions; skipping verification");
            return Ok(());
        };
        if actual != self.resolution {
            return Err(RtspVideoSourceError::ResolutionMismatch {
                configured: self.resolution,
                actual,
            });
        }
        Ok(())
    }
}

impl Drop for RtspVideoSource {
    fn drop(&mut self) {
        // Best-effort TEARDOWN so servers with session limits release this
        // session immediately instead of waiting out its timeout. The reply
        // is intentionally not awaited.
        let uri = self.aggregate_control_url.clone();
        let session_id = self.session_id.clone();
        let _ = self.client.write_request("TEARDOWN", &uri, &[("Session", session_id.as_str())]);
    }
}

impl EncodedVideoSource for RtspVideoSource {
    fn resolution(&self) -> VideoResolution {
        self.resolution
    }

    fn codec(&self) -> EncodedVideoCodec {
        self.codec
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        if self.eof {
            return Ok(None);
        }
        if let Some(pending) = self.pending.take() {
            return Ok(Some(pending));
        }

        // Every read is bounded by the client's socket read timeout, so the
        // stop token, the keepalive timer, and the idle limit are all
        // observed within roughly 100ms even while the stream is silent.
        loop {
            if stop.is_stopped() {
                return Ok(None);
            }
            match self.poll_access_unit().map_err(SourceError::new)? {
                Poll::AccessUnit(access_unit) => {
                    self.verify_resolution(&access_unit).map_err(SourceError::new)?;
                    return Ok(Some(access_unit));
                }
                Poll::TimedOut => {}
                Poll::EndOfStream => {
                    self.eof = true;
                    return Ok(None);
                }
            }
        }
    }

    // TODO: Implement `request_keyframe` by sending a best-effort RTCP PLI
    // on the interleaved RTCP channel (`rtp_channel + 1`), so late
    // subscribers get a keyframe immediately instead of waiting out the
    // producer's keyframe interval.
}

/// Applies the config's per-field credential overrides to the URL userinfo.
fn merge_credentials(
    config: &RtspVideoSourceConfig,
    url: &RtspUrl,
) -> Option<RtspCredentials> {
    let (url_username, url_password) = match &url.credentials {
        Some(credentials) => {
            (Some(credentials.username.clone()), Some(credentials.password.clone()))
        }
        None => (None, None),
    };
    let username = config.username.clone().or(url_username)?;
    let password = config.password.clone().or(url_password).unwrap_or_default();
    Some(RtspCredentials { username, password })
}

fn duration_ms(ms: Option<u32>, default: Duration) -> Duration {
    ms.map(|ms| Duration::from_millis(ms.into())).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    use super::*;

    const SDP_H264: &str = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 H264/90000\r\n";

    const SDP_VP8: &str = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 VP8/90000\r\n";

    fn config(url: String) -> RtspVideoSourceConfig {
        RtspVideoSourceConfig {
            url,
            username: None,
            password: None,
            codec: None,
            resolution: Some(VideoResolution::new(640, 480)),
            connect_timeout_ms: Some(2_000),
            idle_timeout_ms: None,
        }
    }

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

    /// Answers DESCRIBE/SETUP/PLAY on `stream` and returns once the session
    /// is playing on interleaved channels 0-1.
    fn serve_handshake(stream: &mut TcpStream, sdp: &str) {
        let describe = read_request(stream);
        assert!(describe.starts_with("DESCRIBE rtsp://"));
        write_response(
            stream,
            1,
            &[("Content-Type", "application/sdp"), ("Content-Length", &sdp.len().to_string())],
            sdp.as_bytes(),
        );

        let setup = read_request(stream);
        assert!(setup.starts_with("SETUP rtsp://"));
        assert!(setup.contains("Transport: RTP/AVP/TCP;unicast;interleaved=0-1"));
        write_response(
            stream,
            2,
            &[
                ("Session", "abc123;timeout=60"),
                ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
            ],
            &[],
        );

        let play = read_request(stream);
        assert!(play.starts_with("PLAY rtsp://"));
        assert!(play.contains("Session: abc123"));
        write_response(stream, 3, &[], &[]);
    }

    #[test]
    fn connects_and_reads_rtsp_access_unit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let describe = read_request(&mut stream);
            assert!(describe.starts_with("DESCRIBE rtsp://"));
            write_response(
                &mut stream,
                1,
                &[
                    ("Content-Type", "application/sdp"),
                    ("Content-Length", &SDP_H264.len().to_string()),
                ],
                SDP_H264.as_bytes(),
            );

            let setup = read_request(&mut stream);
            assert!(setup.starts_with("SETUP rtsp://"));
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
            assert!(play.contains("Range: npt=0.000-"));
            write_response(&mut stream, 3, &[], &[]);

            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(2, &packet)).unwrap();
        });

        let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            codec: Some(EncodedVideoCodec::H264),
            ..config(format!("rtsp://{addr}/camera"))
        })
        .unwrap();
        assert_eq!(source.codec(), EncodedVideoCodec::H264);
        assert_eq!(source.resolution(), VideoResolution::new(640, 480));
        assert_eq!(source.session_id, "abc123");
        assert_eq!(source.rtp_channel, 2);

        let stop = PumpStop::new();
        let access_unit = source.next_access_unit(&stop).unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        drop(source);
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
            assert!(!second_describe.contains("admin:secret@"));
            assert!(second_describe.contains("Authorization: Digest username=\"admin\""));
            assert!(second_describe.contains(&format!("uri=\"rtsp://{addr}/camera\"")));
            assert!(second_describe.contains("qop=auth"));
            write_response(
                &mut stream,
                2,
                &[
                    ("Content-Type", "application/sdp"),
                    ("Content-Length", &SDP_H264.len().to_string()),
                ],
                SDP_H264.as_bytes(),
            );

            let setup = read_request(&mut stream);
            assert!(setup.contains("Authorization: Digest username=\"admin\""));
            write_response(
                &mut stream,
                3,
                &[
                    ("Session", "abc123;timeout=60"),
                    ("Transport", "RTP/AVP/TCP;unicast;interleaved=0-1"),
                ],
                &[],
            );

            let play = read_request(&mut stream);
            assert!(play.contains("Authorization: Digest username=\"admin\""));
            write_response(&mut stream, 4, &[], &[]);

            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(0, &packet)).unwrap();
        });

        let mut source = RtspVideoSource::new_blocking(config(format!(
            "rtsp://admin:secret@{addr}/camera"
        )))
        .unwrap();

        let stop = PumpStop::new();
        let access_unit = source.next_access_unit(&stop).unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn discovers_resolution_from_first_keyframe() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_VP8);

            // VP8 keyframe with a 10-byte uncompressed header for 640x480.
            let vp8_keyframe = [
                0x10, // payload descriptor: start of partition 0
                0x00, 0x00, 0x00, // frame tag: keyframe
                0x9d, 0x01, 0x2a, // start code
                0x80, 0x02, // width 640
                0xe0, 0x01, // height 480
            ];
            let packet = rtp_packet(10, 12_000, true, &vp8_keyframe);
            stream.write_all(&interleaved(0, &packet)).unwrap();
            read_request(&mut stream) // TEARDOWN
        });

        let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            resolution: None,
            ..config(format!("rtsp://{addr}/camera"))
        })
        .unwrap();
        assert_eq!(source.resolution(), VideoResolution::new(640, 480));

        // The discovery keyframe is handed out first, stamped with the
        // discovered resolution.
        let stop = PumpStop::new();
        let access_unit = source.next_access_unit(&stop).unwrap().unwrap();
        assert_eq!(access_unit.frame_type, EncodedFrameType::Key);
        assert_eq!(access_unit.resolution, VideoResolution::new(640, 480));
        drop(source);

        let teardown = server.join().unwrap();
        assert!(teardown.starts_with("TEARDOWN rtsp://"));
        assert!(teardown.contains("Session: abc123"));
    }

    #[test]
    fn uses_sdp_framesize_without_waiting_for_media() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=0\r\n\
a=rtpmap:96 H264/90000\r\n\
a=framesize:96 1280-720\r\n";
            serve_handshake(&mut stream, sdp);
            // Send no media: construction must not wait for any.
            read_request(&mut stream) // TEARDOWN
        });

        let source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            resolution: None,
            ..config(format!("rtsp://{addr}/camera"))
        })
        .unwrap();
        assert_eq!(source.resolution(), VideoResolution::new(1280, 720));
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn sends_keepalive_during_stream_silence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_H264);

            // Send no interleaved data; the keepalive must arrive during the
            // silence. Only then reply and send the first video frame.
            let keepalive = read_request(&mut stream);
            write_response(&mut stream, 4, &[], &[]);
            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            stream.write_all(&interleaved(0, &packet)).unwrap();
            keepalive
        });

        let mut source =
            RtspVideoSource::new_blocking(config(format!("rtsp://{addr}/camera"))).unwrap();
        source.keepalive_due = Instant::now() + Duration::from_millis(250);

        let stop = PumpStop::new();
        let access_unit = source.next_access_unit(&stop).unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        drop(source);

        let keepalive = server.join().unwrap();
        assert!(keepalive.starts_with("OPTIONS rtsp://"));
        assert!(keepalive.contains("Session: abc123"));
    }

    #[test]
    fn failed_keepalive_reply_surfaces_as_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_H264);
            // The session died server-side: answer in-band with 454.
            write_status_response(&mut stream, 4, &[], &[], 454, "Session Not Found");
            thread::sleep(Duration::from_millis(200));
        });

        let mut source =
            RtspVideoSource::new_blocking(config(format!("rtsp://{addr}/camera"))).unwrap();

        let stop = PumpStop::new();
        let err = source.next_access_unit(&stop).unwrap_err();
        assert!(err.to_string().contains("454"));
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn recovers_interleaved_framing_across_read_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_H264);

            let packet = rtp_packet(10, 12_000, true, &[0x65, 1, 2]);
            let frame = interleaved(0, &packet);
            // Split inside the 4-byte interleaved header and pause long
            // enough for several client read timeouts in between.
            let (head, tail) = frame.split_at(2);
            stream.write_all(head).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(350));
            stream.write_all(tail).unwrap();
            stream.flush().unwrap();
        });

        let mut source =
            RtspVideoSource::new_blocking(config(format!("rtsp://{addr}/camera"))).unwrap();

        let stop = PumpStop::new();
        let access_unit = source.next_access_unit(&stop).unwrap().unwrap();
        assert_eq!(access_unit.payload.as_ref(), &[0, 0, 0, 1, 0x65, 1, 2]);
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn stream_silence_times_out_after_idle_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_H264);
            // Stay silent past the client's idle limit.
            thread::sleep(Duration::from_millis(700));
        });

        let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            idle_timeout_ms: Some(300),
            ..config(format!("rtsp://{addr}/camera"))
        })
        .unwrap();

        let stop = PumpStop::new();
        let err = source.next_access_unit(&stop).unwrap_err();
        assert!(err.to_string().contains("stream data"), "unexpected error: {err}");
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn stop_token_is_observed_during_stream_silence() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            serve_handshake(&mut stream, SDP_H264);
            // Stay silent; the client must return on its stop token alone.
            thread::sleep(Duration::from_millis(700));
        });

        let mut source =
            RtspVideoSource::new_blocking(config(format!("rtsp://{addr}/camera"))).unwrap();

        let stop = PumpStop::new();
        let stop_signal = stop.clone();
        let stopper = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            stop_signal.stop();
        });

        let started = Instant::now();
        let result = source.next_access_unit(&stop).unwrap();
        assert!(result.is_none());
        assert!(
            started.elapsed() < Duration::from_millis(600),
            "stop took {:?}",
            started.elapsed()
        );
        stopper.join().unwrap();
        drop(source);
        server.join().unwrap();
    }

    #[test]
    fn handshake_read_timeout_is_hard_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _describe = read_request(&mut stream);
            // Never respond; hold the connection open past the deadline.
            thread::sleep(Duration::from_millis(600));
        });

        let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            connect_timeout_ms: Some(300),
            ..config(format!("rtsp://{addr}/camera"))
        })
        .unwrap_err();

        assert!(
            err.to_string().contains("DESCRIBE"),
            "expected DESCRIBE timeout, got: {err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn explicit_credentials_override_url_userinfo() {
        let url = RtspUrl::parse("rtsp://old:stale@camera.example/live").unwrap();
        let overriding = RtspVideoSourceConfig {
            username: Some("new".to_owned()),
            password: None,
            ..config("rtsp://camera.example/live".to_owned())
        };

        let credentials = merge_credentials(&overriding, &url).unwrap();
        assert_eq!(credentials.username, "new");
        // The URL password fills the unset field.
        assert_eq!(credentials.password, "stale");

        let no_credentials = merge_credentials(
            &RtspVideoSourceConfig { username: None, password: None, ..overriding },
            &RtspUrl::parse("rtsp://camera.example/live").unwrap(),
        );
        assert!(no_credentials.is_none());
    }
}
