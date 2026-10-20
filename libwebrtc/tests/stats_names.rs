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

//! Reads libwebrtc stats the way `PeerConnection::get_stats` does: the JSON array that
//! `RTCStatsReport::ToJson()` produces, parsed into [`RtcStats`]. The key spellings below are the
//! ones libwebrtc emits (see `api/stats/rtcstats_objects.h`), which follow the W3C spec.

use libwebrtc::stats::RtcStats;

const REPORT: &str = r#"[
  {
    "type": "outbound-rtp",
    "id": "OTV1234",
    "timestamp": 1792450246029000,
    "ssrc": 1234,
    "kind": "video",
    "transportId": "T01",
    "codecId": "COTQ_111_",
    "packetsSent": 4310,
    "bytesSent": 3921004,
    "mid": "1",
    "rid": "r1",
    "frameWidth": 960,
    "frameHeight": 540,
    "framesPerSecond": 30.0,
    "framesEncoded": 1800,
    "encoderImplementation": "libvpx",
    "active": true,
    "scalabilityMode": "L1T3"
  },
  {
    "type": "inbound-rtp",
    "id": "ITV5678",
    "timestamp": 1792450246029000,
    "ssrc": 5678,
    "kind": "video",
    "transportId": "T01",
    "packetsReceived": 4100,
    "bytesReceived": 3700000,
    "framesDecoded": 1750,
    "freezeCount": 3,
    "pauseCount": 0,
    "totalPausesDuration": 0.0,
    "totalFreezesDuration": 1.25
  }
]"#;

#[test]
fn reads_total_freezes_duration_and_scalability_mode() {
    let stats: Vec<RtcStats> = serde_json::from_str(REPORT).expect("report parses");

    let outbound = stats
        .iter()
        .find_map(|s| match s {
            RtcStats::OutboundRtp(o) => Some(o),
            _ => None,
        })
        .expect("outbound-rtp present");
    let inbound = stats
        .iter()
        .find_map(|s| match s {
            RtcStats::InboundRtp(i) => Some(i),
            _ => None,
        })
        .expect("inbound-rtp present");

    assert_eq!(outbound.outbound.scalibility_mode, "L1T3");
    assert_eq!(inbound.inbound.total_freeze_duration, 1.25);
    assert_eq!(inbound.inbound.freeze_count, 3);
}
