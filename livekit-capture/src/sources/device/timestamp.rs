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

//! Capture-timestamp helpers shared by the device capture backends.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum age a backend-reported capture timestamp can have, relative to
/// the wall-clock read time, before it is discarded as stale.
pub(super) const MAX_CAPTURE_TIMESTAMP_AGE_US: u64 = 5_000_000;

/// Returns the current UNIX wall-clock time in microseconds.
pub(super) fn unix_time_us_now() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_micros()).ok()
}

/// Converts a duration to whole microseconds, saturating at `i64::MAX`.
pub(super) fn elapsed_us(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

/// Validates a backend-reported capture timestamp against the wall-clock read
/// time: zero, future, and stale (older than
/// [`MAX_CAPTURE_TIMESTAMP_AGE_US`]) timestamps are rejected.
pub(super) fn validate_capture_timestamp_us(
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

/// Selects the wall-clock capture time for a frame: the validated
/// backend-reported timestamp when there is one, the read time otherwise.
#[cfg(target_os = "linux")]
pub(super) fn select_capture_wall_time_us(
    backend_capture_timestamp: Option<Duration>,
    fallback_wall_time_us: u64,
    read_wall_time_us: u64,
) -> u64 {
    backend_capture_timestamp
        .and_then(|timestamp| u64::try_from(timestamp.as_micros()).ok())
        .and_then(|timestamp_us| validate_capture_timestamp_us(timestamp_us, read_wall_time_us))
        .unwrap_or(fallback_wall_time_us)
}

/// Rebases a `CLOCK_MONOTONIC` frame timestamp onto the wall clock using a
/// paired sampling of both clocks.
#[cfg(target_os = "linux")]
pub(super) fn monotonic_timestamp_to_wallclock(
    frame_timestamp: Duration,
    monotonic_now: Duration,
    wall_now: Duration,
) -> Option<Duration> {
    let frame_age = monotonic_now.checked_sub(frame_timestamp)?;
    wall_now.checked_sub(frame_age)
}

/// Reads a clock via `clock_gettime`.
#[cfg(target_os = "linux")]
pub(super) fn clock_time(clock_id: libc::clockid_t) -> Option<Duration> {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `time` is a valid out pointer and `clock_id` is supplied by libc constants.
    let ret = unsafe { libc::clock_gettime(clock_id, &mut time) };
    if ret != 0 || time.tv_sec < 0 || time.tv_nsec < 0 {
        return None;
    }

    Some(Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
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
