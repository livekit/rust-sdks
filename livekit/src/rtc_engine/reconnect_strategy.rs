// Copyright 2025 LiveKit, Inc.
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

//! Reconnect backoff schedule.
//!
//! Matches livekit-client's `DefaultReconnectPolicy`: after the `n`-th failed
//! attempt the engine waits `min(RECONNECT_MAX_DELAY, RECONNECT_BASE_DELAY * n^2)`
//! (0.3 s, 1.2 s, 2.7 s, 4.8 s, then 7 s), plus up to 1 s of jitter from the
//! second wait on. The waits add up to at least ~44 s, so a client rides out an
//! outage of that length before giving up; the previous full-jitter schedule
//! sampled each wait from `[0, nominal]` and could spend every attempt within a
//! few seconds.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum number of reconnect attempts before the engine gives up and closes.
pub const RECONNECT_ATTEMPTS: u32 = 10;

/// First wait of the schedule; later waits grow with the square of the attempt.
pub const RECONNECT_BASE_DELAY: Duration = Duration::from_millis(300);
/// Kept for API compatibility; the schedule grows quadratically, as in JS.
pub const RECONNECT_BACKOFF_MULTIPLIER: u64 = 2;
/// Cap on any single wait, before jitter.
pub const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(7);

/// Maximum jitter added to every wait after the first.
const RECONNECT_JITTER: Duration = Duration::from_secs(1);

/// Un-jittered wait after the given 1-based failed attempt:
/// `min(RECONNECT_MAX_DELAY, RECONNECT_BASE_DELAY * attempt^2)`.
pub(super) fn nominal(attempt: u32) -> Duration {
    let base = RECONNECT_BASE_DELAY.as_millis() as u64;
    let cap = RECONNECT_MAX_DELAY.as_millis() as u64;
    let n = attempt.max(1) as u64;
    Duration::from_millis(base.saturating_mul(n.saturating_mul(n)).min(cap))
}

/// Wait after the given 1-based failed attempt: `nominal(attempt)` plus, from
/// the second attempt on, jitter uniform in `[0, RECONNECT_JITTER)`. A
/// dependency-free pseudo-random source from the system clock is sufficient;
/// jitter only has to de-correlate clients.
pub(super) fn delay(attempt: u32) -> Duration {
    if attempt <= 1 {
        return nominal(attempt);
    }
    let seed =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    nominal(attempt) + Duration::from_millis(seed % RECONNECT_JITTER.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_matches_the_js_default_reconnect_policy() {
        // livekit-client DEFAULT_RETRY_DELAYS_IN_MS after the first attempt.
        let js = [300, 1200, 2700, 4800, 7000, 7000, 7000, 7000, 7000];
        for (i, want) in js.iter().enumerate() {
            assert_eq!(nominal(i as u32 + 1), Duration::from_millis(*want), "attempt {}", i + 1);
        }
        assert_eq!(nominal(u32::MAX), RECONNECT_MAX_DELAY);
    }

    #[test]
    fn jitter_stays_within_one_second_and_skips_the_first_wait() {
        for _ in 0..1000 {
            assert_eq!(delay(1), nominal(1));
            for attempt in 2..=RECONNECT_ATTEMPTS {
                let d = delay(attempt);
                assert!(d >= nominal(attempt) && d < nominal(attempt) + RECONNECT_JITTER);
            }
        }
    }

    #[test]
    fn budget_outlasts_a_fifteen_second_outage() {
        let floor: Duration = (1..RECONNECT_ATTEMPTS).map(nominal).sum();
        assert!(floor >= Duration::from_secs(40), "minimum reconnect budget is only {floor:?}");
    }
}
