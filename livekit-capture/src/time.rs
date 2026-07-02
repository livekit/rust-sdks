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

//! Shared capture-timestamp helpers used by the capture backends.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum age a backend-reported capture timestamp may have, relative to the
/// wall-clock read time, before it is considered stale and discarded.
pub(crate) const MAX_CAPTURE_TIMESTAMP_AGE_US: u64 = 5_000_000;

/// Returns the current UNIX wall-clock time in microseconds.
pub(crate) fn unix_time_us_now() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_micros()).ok()
}

/// Converts a duration to whole microseconds, saturating at `i64::MAX`.
pub(crate) fn elapsed_us(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

/// Validates a backend-reported capture timestamp against the wall-clock read
/// time: zero, future, and stale (older than
/// [`MAX_CAPTURE_TIMESTAMP_AGE_US`]) timestamps are rejected.
pub(crate) fn validate_capture_timestamp_us(
    capture_timestamp_us: u64,
    read_wall_time_us: u64,
) -> Option<u64> {
    if capture_timestamp_us == 0 || capture_timestamp_us > read_wall_time_us {
        return None;
    }
    if read_wall_time_us - capture_timestamp_us > MAX_CAPTURE_TIMESTAMP_AGE_US {
        return None;
    }
    Some(capture_timestamp_us)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_zero_future_and_stale_timestamps() {
        let now = 10_000_000;
        assert_eq!(validate_capture_timestamp_us(0, now), None);
        assert_eq!(validate_capture_timestamp_us(now + 1, now), None);
        assert_eq!(
            validate_capture_timestamp_us(now - MAX_CAPTURE_TIMESTAMP_AGE_US - 1, now),
            None
        );
        assert_eq!(validate_capture_timestamp_us(now - 1, now), Some(now - 1));
    }
}
