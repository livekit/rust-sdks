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

#[derive(Debug, Clone, Default)]
struct PartialVideoTrack {
    payload_types: Vec<u8>,
    rtp_maps: Vec<SdpRtpMap>,
    fmtps: Vec<(u8, String)>,
    framesizes: Vec<(u8, VideoResolution)>,
    control: Option<String>,
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
    sdp: &str,
    expected_codec: Option<EncodedVideoCodec>,
) -> Result<SdpSession, RtspVideoSourceError> {
    let mut session_control = None;
    let mut tracks = Vec::new();
    let mut current: Option<PartialVideoTrack> = None;
    let mut in_media_section = false;

    for line in sdp.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(media) = line.strip_prefix("m=") {
            if let Some(track) = current.take() {
                tracks.push(track);
            }
            in_media_section = true;
            if let Some(video) = media.strip_prefix("video ") {
                current = Some(parse_video_media(video));
            }
            continue;
        }

        if !in_media_section {
            if let Some(control) = line.strip_prefix("a=control:") {
                session_control = Some(control.trim().to_owned());
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
        } else if let Some(fmtp) = line.strip_prefix("a=fmtp:") {
            if let Some((payload_type, params)) = fmtp.trim().split_once(char::is_whitespace) {
                if let Ok(payload_type) = payload_type.parse() {
                    track.fmtps.push((payload_type, params.to_owned()));
                }
            }
        } else if let Some(framesize) = line.strip_prefix("a=framesize:") {
            if let Some(parsed) = parse_framesize(framesize) {
                track.framesizes.push(parsed);
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

            let parameter_sets = track
                .fmtps
                .iter()
                .find(|(fmtp_payload_type, _)| fmtp_payload_type == payload_type)
                .map(|(_, params)| parse_fmtp_parameter_sets(rtp_map.codec, params))
                .unwrap_or_default();
            let framesize = track
                .framesizes
                .iter()
                .find(|(framesize_payload_type, _)| framesize_payload_type == payload_type)
                .map(|(_, resolution)| *resolution);

            return Ok(SdpSession {
                video: SdpVideoTrack {
                    codec: rtp_map.codec,
                    payload_type: *payload_type,
                    clock_rate: rtp_map.clock_rate,
                    control_url: resolve_control_url(base_url, track.control.as_deref()),
                    parameter_sets,
                    framesize,
                },
                aggregate_control_url: resolve_control_url(base_url, session_control.as_deref()),
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

fn parse_video_media(media: &str) -> PartialVideoTrack {
    let payload_types = media
        .split_whitespace()
        .skip(2)
        .filter_map(|payload_type| payload_type.parse().ok())
        .collect();
    PartialVideoTrack { payload_types, ..Default::default() }
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

/// Resolves an SDP `control` attribute against the session base URL.
fn resolve_control_url(base_url: &str, control: Option<&str>) -> String {
    let Some(control) = control.map(str::trim).filter(|control| !control.is_empty()) else {
        return base_url.to_owned();
    };
    if control == "*" {
        return base_url.to_owned();
    }
    if control.starts_with("rtsp://") {
        return control.to_owned();
    }
    if control.starts_with('/') {
        let authority = base_url
            .strip_prefix("rtsp://")
            .map(|rest| rest.split('/').next().unwrap_or(rest))
            .unwrap_or_default();
        return format!("rtsp://{authority}{control}");
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

        let session = parse_sdp_session(BASE_URL, sdp, Some(EncodedVideoCodec::H264)).unwrap();

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

            let session = parse_sdp_session(BASE_URL, &sdp, Some(codec)).unwrap();

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

        let err = parse_sdp_session(BASE_URL, sdp, Some(EncodedVideoCodec::AV1)).unwrap_err();

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

        let session = parse_sdp_session(BASE_URL, sdp, Some(EncodedVideoCodec::H264)).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, Some(EncodedVideoCodec::H264)).unwrap();

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

        let err = parse_sdp_session(BASE_URL, sdp, Some(EncodedVideoCodec::VP8)).unwrap_err();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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

        let session = parse_sdp_session(BASE_URL, sdp, None).unwrap();

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
    fn resolves_control_against_content_base() {
        let sdp = "\
v=0\r\n\
m=video 0 RTP/AVP 96\r\n\
a=control:trackID=1\r\n\
a=rtpmap:96 H264/90000\r\n";

        let session =
            parse_sdp_session("rtsp://camera.example/relocated/", sdp, None).unwrap();

        assert_eq!(session.video.control_url, "rtsp://camera.example/relocated/trackID=1");
    }
}
