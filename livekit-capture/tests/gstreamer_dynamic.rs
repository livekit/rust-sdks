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

#![cfg(all(feature = "source-gstreamer-dynamic", not(feature = "source-gstreamer")))]

use livekit::webrtc::video_source::EncodedRateControl;
use livekit_capture::{
    encoded::{EncodedFrameType, EncodedVideoCodec, EncodedVideoSource},
    primitive::VideoResolution,
    pump::PumpStop,
    sources::gstreamer::{
        GStreamerBitrateUnit, GStreamerRateControlConfig, GStreamerVideoSource,
        GStreamerVideoSourceConfig,
    },
};

fn config(pipeline: &str) -> GStreamerVideoSourceConfig {
    GStreamerVideoSourceConfig {
        pipeline: pipeline.to_owned(),
        codec: None,
        resolution: None,
        rate_control: Some(GStreamerRateControlConfig {
            element: "lk_encoder".to_owned(),
            property: "target-bitrate".to_owned(),
            unit: GStreamerBitrateUnit::BitsPerSecond,
        }),
    }
}

#[test]
#[ignore = "needs GStreamer and the shim: build livekit-capture-gstreamer, point \
            LIVEKIT_CAPTURE_GSTREAMER_LIBRARY at it, and run with --ignored"]
fn shim_sources_stream_and_close_repeatedly() {
    for _ in 0..3 {
        let mut source = GStreamerVideoSource::new_blocking(config(
            "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=30/1 \
             ! vp8enc name=lk_encoder deadline=1",
        ))
        .unwrap();
        assert_eq!(source.codec(), EncodedVideoCodec::VP8);
        assert_eq!(source.resolution(), VideoResolution::new(320, 240));

        let stop = PumpStop::new();
        let first = source.next_access_unit(&stop).unwrap().expect("stream ended");
        assert_eq!(first.frame_type, EncodedFrameType::Key);
        assert!(!first.payload.is_empty());

        source.update_rate_control(EncodedRateControl {
            target_bitrate_bps: 300_000,
            framerate_fps: 30.0,
        });
        source.request_keyframe();
        for _ in 0..10 {
            source.next_access_unit(&stop).unwrap().expect("stream ended");
        }

        stop.stop();
        assert!(source.next_access_unit(&stop).unwrap().is_none());
    }

    let err = GStreamerVideoSource::new_blocking(config("lk_no_such_element")).unwrap_err();
    assert!(err.to_string().contains("failed to create pipeline"), "{err}");
}
