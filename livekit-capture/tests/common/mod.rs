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

//! Shared helpers for the livekit-capture integration tests.
//!
//! Source-specific helpers (test servers, pipelines) live in a submodule
//! per source, gated by that source's test feature.

#[cfg(feature = "__test-source-rtsp")]
pub mod rtsp;

use std::time::{Duration, Instant};

use livekit_capture::{
    encoded::{EncodedVideoSource, OwnedEncodedAccessUnit},
    pump::PumpStop,
};

/// How long one test waits for its access units before failing.
const PULL_DEADLINE: Duration = Duration::from_secs(15);

/// Pulls `count` access units from an encoded source, failing the test if
/// the stream errors, ends, or stalls.
pub fn pull_access_units(
    source: &mut impl EncodedVideoSource,
    count: usize,
) -> Vec<OwnedEncodedAccessUnit> {
    let stop = PumpStop::new();
    let deadline = Instant::now() + PULL_DEADLINE;
    let mut access_units = Vec::with_capacity(count);
    while access_units.len() < count {
        assert!(
            Instant::now() < deadline,
            "timed out after {} of {count} access units",
            access_units.len()
        );
        match source.next_access_unit(&stop).expect("source failed") {
            Some(access_unit) => {
                log::debug!(
                    "pulled access unit {}/{count}: {:?} {:?}, {} bytes, ts {}us",
                    access_units.len() + 1,
                    access_unit.codec,
                    access_unit.frame_type,
                    access_unit.payload.len(),
                    access_unit.timestamp_us,
                );
                access_units.push(access_unit);
            }
            None => panic!("stream ended after {} of {count} access units", access_units.len()),
        }
    }
    access_units
}
