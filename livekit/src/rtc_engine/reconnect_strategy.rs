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
//! Computes the delay between reconnect attempts as exponential backoff with
//! full jitter. This replaces a previously fixed reconnect interval: it recovers
//! faster from transient blips and spreads retries to avoid synchronised
//! reconnect storms across many clients after a server hiccup.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Maximum number of reconnect attempts before the engine gives up and closes.
pub const RECONNECT_ATTEMPTS: u32 = 10;

/// Exponential-backoff-with-full-jitter parameters for spacing reconnect
/// attempts. The per-attempt delay is sampled uniformly from
/// `[0, min(RECONNECT_MAX_DELAY, RECONNECT_BASE_DELAY * MULTIPLIER^(attempt-1))]`.
pub const RECONNECT_BASE_DELAY: Duration = Duration::from_millis(300);
pub const RECONNECT_BACKOFF_MULTIPLIER: u64 = 2;
pub const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(7);

/// Un-jittered backoff ceiling for the given 1-based reconnect attempt:
/// `min(RECONNECT_MAX_DELAY, RECONNECT_BASE_DELAY * MULTIPLIER^(attempt-1))`,
/// floored at 1ms. Grows geometrically until it saturates at the cap.
pub(super) fn nominal(attempt: u32) -> Duration {
    let base = RECONNECT_BASE_DELAY.as_millis() as u64;
    let cap = RECONNECT_MAX_DELAY.as_millis() as u64;
    let exp = RECONNECT_BACKOFF_MULTIPLIER.saturating_pow(attempt.saturating_sub(1));
    Duration::from_millis(base.saturating_mul(exp).min(cap).max(1))
}

/// Full-jitter backoff delay for the given 1-based reconnect attempt: sampled
/// uniformly from `[0, nominal(attempt)]`. A dependency-free pseudo-random
/// source from the system clock is sufficient — backoff jitter does not need
/// cryptographic quality, only de-correlation across clients.
pub(super) fn delay(attempt: u32) -> Duration {
    let nominal = nominal(attempt).as_millis() as u64;
    let seed =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    Duration::from_millis(seed % (nominal + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_nominal_grows_geometrically_then_caps() {
        // attempt 1 == base, then x2 each step, until it saturates at the cap.
        assert_eq!(nominal(1), RECONNECT_BASE_DELAY);
        assert_eq!(nominal(2), RECONNECT_BASE_DELAY * RECONNECT_BACKOFF_MULTIPLIER as u32);
        assert_eq!(
            nominal(3),
            RECONNECT_BASE_DELAY
                * (RECONNECT_BACKOFF_MULTIPLIER * RECONNECT_BACKOFF_MULTIPLIER) as u32
        );

        // Monotonic non-decreasing and never above the cap.
        let mut prev = Duration::ZERO;
        for attempt in 1..=RECONNECT_ATTEMPTS {
            let nominal = nominal(attempt);
            assert!(nominal >= prev, "backoff must not decrease (attempt {attempt})");
            assert!(nominal <= RECONNECT_MAX_DELAY, "backoff must not exceed the cap");
            prev = nominal;
        }

        // Late attempts are pinned to the cap, and large attempt indices don't
        // overflow into a wrapped-around small value.
        assert_eq!(nominal(RECONNECT_ATTEMPTS), RECONNECT_MAX_DELAY);
        assert_eq!(nominal(u32::MAX), RECONNECT_MAX_DELAY);
    }

    #[test]
    fn backoff_delay_stays_within_nominal_jitter_window() {
        // Full jitter: every sample must land within [0, nominal(attempt)].
        for attempt in 1..=RECONNECT_ATTEMPTS {
            let nominal = nominal(attempt);
            for _ in 0..1000 {
                let delay = delay(attempt);
                assert!(
                    delay <= nominal,
                    "jittered delay {delay:?} exceeded nominal {nominal:?} (attempt {attempt})"
                );
            }
        }
    }
}
