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

/// Priority of a track over the network, sent to the server when the track is published.
///
/// Ordered the way firewall rules are: a lower value is more important. Values below
/// [`TrackPriority::HIGH`] are reserved, and [`TrackPriority::new`] raises them to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(test, derive(fake::Dummy))]
pub struct TrackPriority(u8);

impl TrackPriority {
    /// The most important priority a track can have.
    pub const HIGH: Self = Self(64);
    pub const MEDIUM: Self = Self(128);
    pub const LOW: Self = Self(192);
    /// The least important priority a track can have.
    pub const VERY_LOW: Self = Self(254);

    /// A priority from a value, raised to [`TrackPriority::HIGH`] if more important.
    pub fn new(value: u8) -> Self {
        Self(value.max(Self::HIGH.0))
    }

    /// The value sent to the server.
    pub fn value(self) -> u8 {
        self.0
    }

    pub(crate) fn from_wire(value: u32) -> Self {
        Self::new(value.min(u8::MAX as u32) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_raises_reserved_values() {
        assert_eq!(TrackPriority::new(0), TrackPriority::HIGH);
        assert_eq!(TrackPriority::new(63), TrackPriority::HIGH);
        assert_eq!(TrackPriority::new(70).value(), 70);
        assert!(TrackPriority::HIGH < TrackPriority::LOW, "lower is more important");
        assert_eq!(TrackPriority::from_wire(1000).value(), 255);
    }
}
