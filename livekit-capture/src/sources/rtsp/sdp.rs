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

//! SDP parsing for the video track of an RTSP DESCRIBE response.

use base64::{engine::general_purpose, Engine as _};

use super::{rtp::H26xParameterSets, RtspVideoSourceError};
use crate::{encoded::EncodedVideoCodec, primitive::VideoResolution};

/// RTP timestamp clock rate assumed when the `rtpmap` omits one.
const DEFAULT_CLOCK_RATE: u32 = 90_000;

/// The video stream selected from an SDP session description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SdpSession {
    /// Selected video track.
    pub(super) video: SdpVideoTrack,
    /// Aggregate control URL for session-level requests (PLAY, OPTIONS,
    /// TEARDOWN), from the session-level `a=control` attribute.
    pub(super) aggregate_control_url: String,
}

/// A video track selected from the SDP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SdpVideoTrack {
    /// RTP payload codec.
    pub(super) codec: EncodedVideoCodec,
    /// RTP payload type.
    pub(super) payload_type: u8,
    /// RTP timestamp clock rate.
    pub(super) clock_rate: u32,
    /// Media control URL used for SETUP.
    pub(super) control_url: String,
    /// Out-of-band parameter sets from the track's `fmtp` attribute.
    pub(super) parameter_sets: H26xParameterSets,
    /// Resolution from the track's `a=framesize` attribute, when present.
    pub(super) framesize: Option<VideoResolution>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SdpRtpMap {
    payload_type: u8,
    codec: EncodedVideoCodec,
    clock_rate: u32,
}

/// Selects a video track from an SDP session description.
///
/// `base_url` is the URL relative control attributes resolve against: the
/// DESCRIBE response's `Content-Base` when present, else the request URL.
/// With an `expected_codec`, only tracks carrying that codec match; without
/// one, the first supported video track is selected.
pub(super) fn parse_sdp_session(
    base_url: &str,
    sdp: &[u8],
    expected_codec: Option<EncodedVideoCodec>,
) -> Result<SdpSession, RtspVideoSourceError> {
    let session = sdp_types::Session::parse(sdp).map_err(|err| {
        // The parser error can quote server-provided bytes; escape them.
        log::debug!("failed to parse SDP: {}", err.to_string().escape_debug());
        RtspVideoSourceError::InvalidSdp
    })?;
    let session_control = attribute_value(&session.attributes, "control");

    // Selecting among multiple video tracks is not supported (only ordinal
    // selection would have standard SDP footing); surface the choice so it
    // is not hidden.
    let video_tracks = || session.medias.iter().filter(|media| media.media == "video");
    let video_track_count = video_tracks().count();
    if video_track_count > 1 {
        let summary = video_tracks()
            .map(|media| {
                let codecs: Vec<&str> = attribute_values(&media.attributes, "rtpmap")
                    .filter_map(|rtpmap| rtpmap.split_whitespace().nth(1))
                    .filter_map(|encoding| encoding.split('/').next())
                    .collect();
                if codecs.is_empty() { "?".to_owned() } else { codecs.join("+") }
            })
            .collect::<Vec<_>>()
            .join(", ");
        log::info!(
            "RTSP SDP offers {video_track_count} video tracks ({}); \
             using the first one with a supported codec",
            super::sanitized(summary),
        );
    }

    let mut offered = Vec::new();
    for media in &session.medias {
        if media.media != "video" {
            continue;
        }
        let rtp_maps: Vec<SdpRtpMap> = attribute_values(&media.attributes, "rtpmap")
            .filter_map(parse_rtpmap)
            .collect();

        for payload_type in media.fmt.split_whitespace().filter_map(|pt| pt.parse::<u8>().ok()) {
            let Some(rtp_map) = rtp_maps.iter().find(|map| map.payload_type == payload_type)
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

            let parameter_sets = attribute_values(&media.attributes, "fmtp")
                .find_map(|value| {
                    let (fmtp_payload_type, params) =
                        value.trim().split_once(char::is_whitespace)?;
                    (fmtp_payload_type.parse::<u8>().ok()? == payload_type)
                        .then(|| parse_fmtp_parameter_sets(rtp_map.codec, params))
                })
                .unwrap_or_default();
            let framesize = attribute_values(&media.attributes, "framesize")
                .filter_map(parse_framesize)
                .find_map(|(framesize_payload_type, resolution)| {
                    (framesize_payload_type == payload_type).then_some(resolution)
                });

            return Ok(SdpSession {
                video: SdpVideoTrack {
                    codec: rtp_map.codec,
                    payload_type,
                    clock_rate: rtp_map.clock_rate,
                    control_url: resolve_control_url(
                        base_url,
                        attribute_value(&media.attributes, "control"),
                    ),
                    parameter_sets,
                    framesize,
                },
                aggregate_control_url: resolve_control_url(base_url, session_control),
            });
        }
    }

    match expected_codec {
        Some(expected) if !offered.is_empty() => {
            Err(RtspVideoSourceError::CodecMismatch { expected, offered })
        }
        _ => Err(RtspVideoSourceError::MissingVideoTrack),
    }
}

/// Returns the first value of the named attribute.
fn attribute_value<'a>(attributes: &'a [sdp_types::Attribute], name: &'a str) -> Option<&'a str> {
    attribute_values(attributes, name).next()
}

/// Returns every value of the named attribute.
fn attribute_values<'a>(
    attributes: &'a [sdp_types::Attribute],
    name: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    attributes
        .iter()
        .filter(move |attribute| attribute.attribute == name)
        .filter_map(|attribute| attribute.value.as_deref())
        .map(str::trim)
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
        .unwrap_or(DEFAULT_CLOCK_RATE);
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

/// Parses an `a=framesize:<payload type> <width>-<height>` value (RFC 6064).
fn parse_framesize(framesize: &str) -> Option<(u8, VideoResolution)> {
    let (payload_type, dimensions) = framesize.trim().split_once(char::is_whitespace)?;
    let payload_type = payload_type.parse().ok()?;
    let (width, height) = dimensions.trim().split_once('-')?;
    let resolution = VideoResolution::new(width.parse().ok()?, height.parse().ok()?);
    Some((payload_type, resolution))
}

/// Decodes out-of-band parameter sets from an `fmtp` parameter list:
/// `sprop-parameter-sets` for H.264, `sprop-vps`/`sprop-sps`/`sprop-pps`
/// for H.265. Individually malformed entries are skipped.
fn parse_fmtp_parameter_sets(codec: EncodedVideoCodec, params: &str) -> H26xParameterSets {
    let mut sets = H26xParameterSets::default();
    for param in params.split(';') {
        let Some((name, value)) = param.trim().split_once('=') else {
            continue;
        };
        match codec {
            EncodedVideoCodec::H264 if name.eq_ignore_ascii_case("sprop-parameter-sets") => {
                for nal in decode_base64_nals(value) {
                    // Classify by NAL type rather than position: the
                    // attribute usually lists SPS then PPS, but not always.
                    match nal.first().map(|header| header & 0x1f) {
                        Some(7) => sets.sps.push(nal),
                        Some(8) => sets.pps.push(nal),
                        _ => {}
                    }
                }
            }
            EncodedVideoCodec::H265 => {
                let target = if name.eq_ignore_ascii_case("sprop-vps") {
                    &mut sets.vps
                } else if name.eq_ignore_ascii_case("sprop-sps") {
                    &mut sets.sps
                } else if name.eq_ignore_ascii_case("sprop-pps") {
                    &mut sets.pps
                } else {
                    continue;
                };
                target.extend(decode_base64_nals(value));
            }
            _ => {}
        }
    }
    sets
}

fn decode_base64_nals(value: &str) -> Vec<Vec<u8>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|encoded| !encoded.is_empty())
        .filter_map(|encoded| general_purpose::STANDARD.decode(encoded).ok())
        .filter(|nal| !nal.is_empty())
        .collect()
}

/// Splits an `rtsp://` or `rtsps://` URL into its scheme and remainder.
fn split_rtsp_scheme(url: &str) -> Option<(&str, &str)> {
    if let Some(rest) = url.strip_prefix("rtsp://") {
        return Some(("rtsp://", rest));
    }
    url.strip_prefix("rtsps://").map(|rest| ("rtsps://", rest))
}

/// Resolves an SDP `control` attribute against the session base URL.
fn resolve_control_url(base_url: &str, control: Option<&str>) -> String {
    let Some(control) = control.map(str::trim).filter(|control| !control.is_empty()) else {
        return base_url.to_owned();
    };
    if control == "*" {
        return base_url.to_owned();
    }
    if split_rtsp_scheme(control).is_some() {
        return control.to_owned();
    }
    if control.starts_with('/') {
        // An absolute path keeps the base URL's scheme and authority.
        let (scheme, rest) = split_rtsp_scheme(base_url).unwrap_or(("rtsp://", base_url));
        let authority = rest.split('/').next().unwrap_or(rest);
        return format!("{scheme}{authority}{control}");
    }
    format!("{}/{}", base_url.trim_end_matches('/'), control)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE_URL: &str = "rtsp://camera.example/live";

    #[test]
    fn parses_sdp_video_track() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(session.video.codec, EncodedVideoCodec::H264);
        assert_eq!(session.video.payload_type, 96);
        assert_eq!(session.video.clock_rate, 90_000);
        assert_eq!(session.video.control_url, "rtsp://camera.example/live/trackID=1");
        assert_eq!(session.aggregate_control_url, BASE_URL);
        assert!(session.video.parameter_sets.is_empty());
        assert_eq!(session.video.framesize, None);
    }

    #[test]
    fn parses_vp8_vp9_and_av1_sdp_video_tracks() {
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

            let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(codec)).unwrap();

            assert_eq!(session.video.codec, codec);
            assert_eq!(session.video.payload_type, 96);
            assert_eq!(session.video.clock_rate, 90_000);
        }
    }

    #[test]
    fn rejects_sdp_codec_mismatch() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 VP9/90000\r\n";

        let err = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(EncodedVideoCodec::AV1)).unwrap_err();

        match err {
            RtspVideoSourceError::CodecMismatch { expected, offered } => {
                assert_eq!(expected, EncodedVideoCodec::AV1);
                assert_eq!(offered, vec![EncodedVideoCodec::VP9]);
            }
            other => panic!("expected codec mismatch, got {other:?}"),
        }
    }

    #[test]
    fn selects_expected_codec_among_multiple_payload_types() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(session.video.codec, EncodedVideoCodec::H264);
        assert_eq!(session.video.payload_type, 96);
    }

    #[test]
    fn selects_expected_codec_from_later_video_section() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=2\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(EncodedVideoCodec::H264)).unwrap();

        assert_eq!(session.video.codec, EncodedVideoCodec::H264);
        assert_eq!(session.video.control_url, "rtsp://camera.example/live/trackID=2");
    }

    #[test]
    fn rejects_sdp_listing_all_offered_codecs_when_none_match() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 98 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:98 H265/90000\r\n\
a=rtpmap:96 H264/90000\r\n";

        let err = parse_sdp_session(BASE_URL, sdp.as_bytes(), Some(EncodedVideoCodec::VP8)).unwrap_err();

        match err {
            RtspVideoSourceError::CodecMismatch { expected, offered } => {
                assert_eq!(expected, EncodedVideoCodec::VP8);
                assert_eq!(offered, vec![EncodedVideoCodec::H265, EncodedVideoCodec::H264]);
            }
            other => panic!("expected codec mismatch, got {other:?}"),
        }
    }

    #[test]
    fn ignores_audio_sections() {
        let sdp = "\
v=0\r\n\
m=audio 0 RTP/AVP 97\r\n\
a=rtpmap:97 PCMU/8000\r\n\
a=control:trackID=1\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=2\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.codec, EncodedVideoCodec::H264);
        assert_eq!(session.video.control_url, "rtsp://camera.example/live/trackID=2");
    }

    #[test]
    fn parses_h264_sprop_parameter_sets() {
        // 0x67 (SPS) and 0x68 (PPS) prefixed NAL units.
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=rtpmap:96 H264/90000\r\n\
a=fmtp:96 packetization-mode=1;sprop-parameter-sets=ZwlA,aAlB;profile-level-id=42e01e\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.parameter_sets.sps, vec![vec![0x67, 0x09, 0x40]]);
        assert_eq!(session.video.parameter_sets.pps, vec![vec![0x68, 0x09, 0x41]]);
        assert!(session.video.parameter_sets.vps.is_empty());
    }

    #[test]
    fn parses_h265_sprop_attributes() {
        // 0x40 (VPS), 0x42 (SPS), and 0x44 (PPS) prefixed NAL units.
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=rtpmap:96 H265/90000\r\n\
a=fmtp:96 sprop-vps=QAEB;sprop-sps=QgEC;sprop-pps=RAED\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.parameter_sets.vps, vec![vec![0x40, 0x01, 0x01]]);
        assert_eq!(session.video.parameter_sets.sps, vec![vec![0x42, 0x01, 0x02]]);
        assert_eq!(session.video.parameter_sets.pps, vec![vec![0x44, 0x01, 0x03]]);
    }

    #[test]
    fn tolerates_malformed_sprop_entries() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=rtpmap:96 H264/90000\r\n\
a=fmtp:96 sprop-parameter-sets=!!!not-base64!!!,ZwlA\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.parameter_sets.sps, vec![vec![0x67, 0x09, 0x40]]);
        assert!(session.video.parameter_sets.pps.is_empty());
    }

    #[test]
    fn parses_framesize_attribute() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=rtpmap:96 H264/90000\r\n\
a=framesize:96 1280-720\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.framesize, Some(VideoResolution::new(1280, 720)));
    }

    #[test]
    fn resolves_session_level_aggregate_control() {
        let sdp = "\
v=0\r\n\
a=control:rtsp://camera.example/live/aggregate\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.aggregate_control_url, "rtsp://camera.example/live/aggregate");
        assert_eq!(session.video.control_url, "rtsp://camera.example/live/trackID=1");
    }

    #[test]
    fn star_control_resolves_to_base_url() {
        let sdp = "\
v=0\r\n\
a=control:*\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session = parse_sdp_session(BASE_URL, sdp.as_bytes(), None).unwrap();

        assert_eq!(session.aggregate_control_url, BASE_URL);
    }

    #[test]
    fn resolves_absolute_path_control_url() {
        assert_eq!(
            resolve_control_url(BASE_URL, Some("/stream/trackID=1")),
            "rtsp://camera.example/stream/trackID=1"
        );
    }

    #[test]
    fn resolves_control_urls_with_rtsps_scheme() {
        // An absolute path keeps the rtsps scheme of the base URL.
        assert_eq!(
            resolve_control_url("rtsps://camera.example:7441/live", Some("/stream/trackID=1")),
            "rtsps://camera.example:7441/stream/trackID=1"
        );
        // A relative control appends to the rtsps base.
        assert_eq!(
            resolve_control_url("rtsps://camera.example:7441/live", Some("trackID=1")),
            "rtsps://camera.example:7441/live/trackID=1"
        );
        // An absolute rtsps URL passes through.
        assert_eq!(
            resolve_control_url(BASE_URL, Some("rtsps://camera.example/other")),
            "rtsps://camera.example/other"
        );
    }

    #[test]
    fn resolves_control_against_content_base() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session =
            parse_sdp_session("rtsp://camera.example/relocated/", sdp.as_bytes(), None).unwrap();

        assert_eq!(session.video.control_url, "rtsp://camera.example/relocated/trackID=1");
    }
}
