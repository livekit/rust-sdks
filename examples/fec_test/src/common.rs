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

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use livekit_api::access_token::{AccessToken, VideoGrants};
use livekit_protocol::RoomConfiguration;

pub fn mint_token(
    api_key: &str,
    api_secret: &str,
    room: &str,
    identity: &str,
    low_latency: bool,
) -> Result<String> {
    let mut token = AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_ttl(Duration::from_secs(3600))
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_owned(),
            can_publish: true,
            can_subscribe: true,
            ..Default::default()
        });
    // Low-latency mode attaches a room configuration that enables the
    // playout-delay RTP extension with ~0 delay (min 0 / max 1 ms). The SFU
    // (re)creates the room with it on connect, so the publisher must connect
    // before any subscriber joins — a subscriber already in the room would be
    // dropped by the re-creation.
    if low_latency {
        token = token.with_room_config(RoomConfiguration {
            name: room.to_owned(),
            min_playout_delay: 0,
            max_playout_delay: 1,
            ..Default::default()
        });
    }
    Ok(token.to_jwt()?)
}

pub fn unix_time_secs() -> f64 {
    // now() is always at or after the epoch, so duration_since never errors
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// Wall-clock microseconds since the epoch. Used as the publisher's per-frame
/// user_timestamp so the subscriber (same host, same clock) can compute true
/// capture-to-decode latency.
pub fn unix_time_micros() -> u64 {
    // now() is always at or after the epoch, so duration_since never errors
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}
