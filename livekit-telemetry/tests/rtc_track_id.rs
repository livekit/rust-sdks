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

use livekit_telemetry::{RtcStatsSample, StreamDirection, TrackKind};

/// Host code can construct and access the voted public track identifier field.
#[test]
fn public_rtc_sample_uses_track_id() {
    let sample = RtcStatsSample::new("TR_camera", TrackKind::Video, StreamDirection::Outbound);
    assert_eq!(sample.track_id, "TR_camera");
    let next = RtcStatsSample { track_id: "TR_microphone".into(), ..sample };
    assert_eq!(next.track_id, "TR_microphone");
}
