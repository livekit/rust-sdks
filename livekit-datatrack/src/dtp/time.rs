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

use rand::Rng;
use std::time::{Duration, Instant};

/// Packet-level timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp<const RATE: u32>(u32);

impl<const RATE: u32> Timestamp<RATE> {
    pub fn random() -> Self {
        Self::from_ticks(rand::rng().random::<u32>())
    }

    pub const fn from_ticks(ticks: u32) -> Self {
        Self(ticks)
    }

    pub const fn as_ticks(&self) -> u32 {
        self.0
    }

    const fn is_before(&self, other: Self) -> bool {
        (self.0.wrapping_sub(other.0) as i32) < 0
    }

    const fn wrapping_add(self, ticks: u32) -> Self {
        Self(self.0.wrapping_add(ticks))
    }
}

/// Monotonic mapping from an epoch to a packet-level timestamp.
#[derive(Debug)]
pub struct Clock<const RATE: u32> {
    epoch: Instant,
    base: Timestamp<RATE>,
    prev: Timestamp<RATE>,
}

impl<const RATE: u32> Clock<RATE> {
    /// Creates a new clock with epoch equal to [`Instant::now()`].
    pub fn new(base: Timestamp<RATE>) -> Self {
        Self::with_epoch(Instant::now(), base)
    }

    /// Creates a new clock with an explicit epoch instant.
    pub fn with_epoch(epoch: Instant, base: Timestamp<RATE>) -> Self {
        Self { epoch, base, prev: base }
    }

    /// Returns the timestamp corresponding to [`Instant::now()`].
    pub fn now(&mut self) -> Timestamp<RATE> {
        self.at(Instant::now())
    }

    /// Returns the timestamp corresponding to the given instant.
    pub fn at(&mut self, instant: Instant) -> Timestamp<RATE> {
        let elapsed = instant.duration_since(self.epoch);
        let ticks = Self::duration_to_ticks(elapsed);

        let mut ts = self.base.wrapping_add(ticks);
        // Enforce monotonicity in RTP wraparound space
        if ts.is_before(self.prev) {
            ts = self.prev;
        }
        self.prev = ts;
        ts
    }

    /// Convert a duration since the epoch into clock ticks.
    const fn duration_to_ticks(duration: Duration) -> u32 {
        // round(nanos * rate_hz / 1e9)
        let nanos = duration.as_nanos();
        let ticks = (nanos * RATE as u128 + 500_000_000) / 1_000_000_000;
        ticks as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type DefaultClock = Clock<90_000>;

    #[test]
    fn test_is_base_at_epoch() {
        let epoch = Instant::now();
        let base = Timestamp::from_ticks(1234);
        let mut clock = DefaultClock::with_epoch(epoch, base);

        assert_eq!(clock.at(epoch).as_ticks(), base.as_ticks());
        assert_eq!(clock.prev.as_ticks(), base.as_ticks());
    }

    #[test]
    fn test_monotonic() {
        let epoch = Instant::now();
        let base = Timestamp::from_ticks(0);
        let mut clock = DefaultClock::with_epoch(epoch, base);

        let t1 = epoch + Duration::from_millis(100);
        let t0 = epoch + Duration::from_millis(50);
        assert_eq!(clock.at(t1).as_ticks(), clock.at(t0).as_ticks(), "Clock went backwards");
    }
}

#[cfg(test)]
impl<const RATE: u32> fake::Dummy<fake::Faker> for Timestamp<RATE> {
    fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
        Self(rng.random())
    }
}