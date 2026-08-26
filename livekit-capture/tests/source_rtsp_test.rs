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
        test_config, RtspTestServer, H264_PIPELINE, H265_PIPELINE, TEST_HEIGHT, TEST_WIDTH,
        VP8_PIPELINE,
    },
};
use livekit_capture::{
    encoded::{h26x::annex_b_nalus, EncodedFrameType, EncodedVideoCodec, EncodedVideoSource},
    primitive::VideoResolution,
    sources::rtsp::{RtspVideoSource, RtspVideoSourceConfig},
};

const TEST_RESOLUTION: VideoResolution = VideoResolution::new(TEST_WIDTH, TEST_HEIGHT);

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
    let server = RtspTestServer::launch(H264_PIPELINE);
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
    let server = RtspTestServer::launch(H264_PIPELINE);
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
    let server = RtspTestServer::launch(H265_PIPELINE);
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
    let server = RtspTestServer::launch(VP8_PIPELINE);
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
    let server = RtspTestServer::launch(H264_PIPELINE);
    let err = RtspVideoSource::new_blocking(RtspVideoSourceConfig {
        codec: Some(EncodedVideoCodec::VP8),
        resolution: Some(TEST_RESOLUTION),
        ..test_config(server.url())
    })
    .unwrap_err();

    assert!(err.to_string().contains("codec mismatch"), "unexpected error: {err}");
}

#[test]
fn authenticates_with_digest() {
    let server = RtspTestServer::launch_with_digest_auth(H264_PIPELINE, "admin", "secret");

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
