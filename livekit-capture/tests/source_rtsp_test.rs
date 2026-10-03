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

//! Integration tests for the RTSP source against an in-process GStreamer
//! RTSP server. Not run by default; see `tests/README.md`.

#![cfg(feature = "__test-source-rtsp")]

mod common;

// Replaces `#[test]` with a variant that initializes logging, so `log::`
// output from the source and the test server is visible; see tests/README.md.
use test_log::test;

use common::{
    pull_access_units,
    rtsp::{
        default_pipeline, default_pipeline_with_audio, pipeline, test_config, RtspTestServer,
        TestCodec, TEST_RESOLUTION,
    },
};
use livekit_capture::{
    encoded::{h26x::annex_b_nalus, EncodedFrameType, EncodedVideoCodec, EncodedVideoSource},
    primitive::VideoResolution,
    sources::rtsp::{RtspVideoSource, RtspVideoSourceConfig},
};

fn h264_nal_types(payload: &[u8]) -> Vec<u8> {
    annex_b_nalus(payload).iter().map(|nal| nal[0] & 0x1f).collect()
}

fn h265_nal_types(payload: &[u8]) -> Vec<u8> {
    annex_b_nalus(payload).iter().map(|nal| (nal[0] >> 1) & 0x3f).collect()
}

fn assert_increasing_timestamps(access_units: &[livekit_capture::encoded::OwnedEncodedAccessUnit]) {
    for window in access_units.windows(2) {
        assert!(
            window[1].timestamp_us > window[0].timestamp_us,
            "timestamps must increase: {} then {}",
            window[0].timestamp_us,
            window[1].timestamp_us,
        );
    }
}

#[test]
fn streams_h264_access_units() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::H264),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .expect("failed to connect");
    assert_eq!(source.codec(), EncodedVideoCodec::H264);
    assert_eq!(source.resolution(), TEST_RESOLUTION);

    let access_units = pull_access_units(&mut source, 15);

    // The first access unit must be a self-contained keyframe: subscribers
    // can only initialize a decoder from parameter sets inside it.
    let first = &access_units[0];
    assert_eq!(first.frame_type, EncodedFrameType::Key);
    let nal_types = h264_nal_types(&first.payload);
    assert!(nal_types.contains(&7), "keyframe missing SPS: {nal_types:?}");
    assert!(nal_types.contains(&8), "keyframe missing PPS: {nal_types:?}");
    assert!(nal_types.contains(&5), "keyframe missing IDR: {nal_types:?}");

    assert_increasing_timestamps(&access_units);
    for access_unit in &access_units {
        assert_eq!(access_unit.codec, EncodedVideoCodec::H264);
        assert!(access_unit.payload.starts_with(&[0, 0, 0, 1]));
        assert_eq!(access_unit.resolution, TEST_RESOLUTION);
    }
}

#[test]
fn discovers_h264_resolution() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    let mut source =
        RtspVideoSource::new_blocking(test_config(server.url())).expect("failed to connect");

    assert_eq!(source.resolution(), TEST_RESOLUTION);

    // The keyframe consumed by discovery must not be lost.
    let first = pull_access_units(&mut source, 1).remove(0);
    assert_eq!(first.frame_type, EncodedFrameType::Key);
    assert_eq!(first.resolution, TEST_RESOLUTION);
}

#[test]
fn streams_h265_access_units() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H265));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::H265),
        ..test_config(server.url())
    })
    .expect("failed to connect");
    assert_eq!(source.resolution(), TEST_RESOLUTION);

    let access_units = pull_access_units(&mut source, 5);

    // H.265 keyframes must carry VPS, SPS, and PPS alongside the IDR — via
    // the stream or injected from the real server's SDP.
    let first = &access_units[0];
    assert_eq!(first.frame_type, EncodedFrameType::Key);
    let nal_types = h265_nal_types(&first.payload);
    for parameter_set in [32u8, 33, 34] {
        assert!(nal_types.contains(&parameter_set), "keyframe missing NAL {parameter_set}");
    }
    assert!(
        nal_types.iter().any(|nal_type| matches!(nal_type, 19 | 20)),
        "keyframe missing IDR: {nal_types:?}"
    );
    assert_increasing_timestamps(&access_units);
}

#[test]
fn streams_vp8_access_units() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::Vp8));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::VP8),
        ..test_config(server.url())
    })
    .expect("failed to connect");

    // VP8 has no SDP resolution hints; discovery parses the first keyframe.
    assert_eq!(source.resolution(), TEST_RESOLUTION);

    let access_units = pull_access_units(&mut source, 5);
    assert_eq!(access_units[0].frame_type, EncodedFrameType::Key);
    assert_increasing_timestamps(&access_units);
    for access_unit in &access_units {
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP8);
        assert!(!access_unit.payload.is_empty());
    }
}

#[test]
fn rejects_codec_mismatch() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::VP8),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();

    assert!(err.to_string().contains("codec mismatch"), "unexpected error: {err}");
}

#[test]
fn streams_h264_over_rtsps() {
    let server = RtspTestServer::launch_tls(&default_pipeline(TestCodec::H264));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::H264),
        resolution: Some(TEST_RESOLUTION),
        // The test server's certificate is self-signed.
        accept_invalid_tls_certs: true,
        ..test_config(server.url())
    })
    .expect("failed to connect over TLS");

    let access_units = pull_access_units(&mut source, 5);
    assert_eq!(access_units[0].frame_type, EncodedFrameType::Key);
    assert_increasing_timestamps(&access_units);
}

#[test]
fn rejects_untrusted_tls_certificate() {
    let server = RtspTestServer::launch_tls(&default_pipeline(TestCodec::H264));
    // Default configuration verifies against the system roots, which must
    // reject the server's self-signed certificate.
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();

    let message = err.to_string();
    assert!(
        message.contains("TLS") || message.contains("certificate"),
        "unexpected error: {message}"
    );
}

#[test]
fn authenticates_with_digest_over_rtsps() {
    let server = RtspTestServer::launch_tls_with_digest_auth(&default_pipeline(TestCodec::H264), "admin", "secret");
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        username: Some("admin".to_owned()),
        password: Some("secret".to_owned()),
        codec: Some(EncodedVideoCodec::H264),
        resolution: Some(TEST_RESOLUTION),
        accept_invalid_tls_certs: true,
        ..test_config(server.url())
    })
    .expect("failed to connect with credentials over TLS");

    let first = pull_access_units(&mut source, 1).remove(0);
    assert_eq!(first.frame_type, EncodedFrameType::Key);
}

#[test]
fn authenticates_with_digest() {
    let server = RtspTestServer::launch_with_digest_auth(&default_pipeline(TestCodec::H264), "admin", "secret");

    // Without credentials the server's challenge cannot be answered.
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();
    assert!(err.to_string().contains("credentials"), "unexpected error: {err}");

    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        username: Some("admin".to_owned()),
        password: Some("secret".to_owned()),
        codec: Some(EncodedVideoCodec::H264),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .expect("failed to connect with credentials");

    let first = pull_access_units(&mut source, 1).remove(0);
    assert_eq!(first.frame_type, EncodedFrameType::Key);
}

#[test]
fn selects_video_track_among_audio() {
    let server = RtspTestServer::launch(&default_pipeline_with_audio(TestCodec::H264));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .expect("failed to connect");

    // The video track is selected from the two-track SDP; the audio track
    // is neither set up nor streamed.
    assert_eq!(source.codec(), EncodedVideoCodec::H264);
    let access_units = pull_access_units(&mut source, 5);
    assert_eq!(access_units[0].frame_type, EncodedFrameType::Key);
    for access_unit in &access_units {
        assert_eq!(access_unit.codec, EncodedVideoCodec::H264);
    }
}

#[test]
fn discovers_cropped_h264_resolution() {
    // 1080p is coded as 1088 rows plus SPS frame cropping; discovery must
    // report the display resolution from a real encoder's SPS.
    let server = RtspTestServer::launch(&pipeline(TestCodec::H264, VideoResolution::new(1920, 1080)));
    let source =
        RtspVideoSource::new_blocking(test_config(server.url())).expect("failed to connect");

    assert_eq!(source.resolution(), VideoResolution::new(1920, 1080));
}

#[test]
fn streams_vp9_access_units() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::Vp9));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::VP9),
        ..test_config(server.url())
    })
    .expect("failed to connect");

    // Discovery parses the first keyframe's uncompressed header.
    assert_eq!(source.resolution(), TEST_RESOLUTION);

    let access_units = pull_access_units(&mut source, 5);
    assert_eq!(access_units[0].frame_type, EncodedFrameType::Key);
    assert_increasing_timestamps(&access_units);
    for access_unit in &access_units {
        assert_eq!(access_unit.codec, EncodedVideoCodec::VP9);
        assert!(!access_unit.payload.is_empty());
    }
}

#[test]
fn streams_av1_access_units() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::Av1));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::AV1),
        ..test_config(server.url())
    })
    .expect("failed to connect");

    // Discovery parses the sequence header OBU of the first keyframe.
    assert_eq!(source.resolution(), TEST_RESOLUTION);

    let access_units = pull_access_units(&mut source, 5);
    assert_eq!(access_units[0].frame_type, EncodedFrameType::Key);
    assert_increasing_timestamps(&access_units);
    for access_unit in &access_units {
        assert_eq!(access_unit.codec, EncodedVideoCodec::AV1);
        assert!(!access_unit.payload.is_empty());
    }
}

#[test]
fn rejects_wrong_credentials() {
    let server = RtspTestServer::launch_with_digest_auth(
        &default_pipeline(TestCodec::H264),
        "admin",
        "secret",
    );
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        username: Some("admin".to_owned()),
        password: Some("wrong".to_owned()),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();

    // One authenticated retry, then a clean failure — no retry loop.
    assert!(err.to_string().contains("401"), "unexpected error: {err}");
}

#[test]
fn rejects_unknown_path() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url().replace("/test", "/wrong"))
    })
    .unwrap_err();

    assert!(err.to_string().contains("404"), "unexpected error: {err}");
}

#[test]
fn rejects_mismatched_declared_resolution() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    // A declared resolution skips discovery and is verified against the
    // first keyframe instead.
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(VideoResolution::new(1280, 720)),
        ..test_config(server.url())
    })
    .expect("failed to connect");

    let stop = livekit_capture::pump::PumpStop::new();
    let err = source.next_access_unit(&stop).unwrap_err();
    assert!(err.to_string().contains("1280x720"), "unexpected error: {err}");
}

#[test]
fn reconnects_after_drop() {
    let server = RtspTestServer::launch(&default_pipeline(TestCodec::H264));
    // The previous source's teardown must leave the server usable for a
    // fresh connection.
    for _ in 0..2 {
        let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
            resolution: Some(TEST_RESOLUTION),
            ..test_config(server.url())
        })
        .expect("failed to connect");
        let first = pull_access_units(&mut source, 1).remove(0);
        assert_eq!(first.frame_type, EncodedFrameType::Key);
    }
}

#[test]
fn authenticates_with_basic() {
    let server = RtspTestServer::launch_with_basic_auth(
        &default_pipeline(TestCodec::H264),
        "admin",
        "secret",
    );

    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();
    assert!(err.to_string().contains("credentials"), "unexpected error: {err}");

    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        username: Some("admin".to_owned()),
        password: Some("secret".to_owned()),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .expect("failed to connect with credentials");

    let first = pull_access_units(&mut source, 1).remove(0);
    assert_eq!(first.frame_type, EncodedFrameType::Key);
}

#[test]
fn streams_h265_over_rtsps() {
    let server = RtspTestServer::launch_tls(&default_pipeline(TestCodec::H265));
    let mut source = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::H265),
        accept_invalid_tls_certs: true,
        ..test_config(server.url())
    })
    .expect("failed to connect over TLS");

    assert_eq!(source.resolution(), TEST_RESOLUTION);
    let first = pull_access_units(&mut source, 1).remove(0);
    assert_eq!(first.frame_type, EncodedFrameType::Key);
}
