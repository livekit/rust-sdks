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

use crate::dtp::{Dtp, E2ee, TrackHandle};
use bytes::Bytes;

/// Converts application-level frames into packets for transport.
pub struct Packetizer {
    track_handle: TrackHandle,
    mtu_size: usize,
    sequence: u16,
    frame_number: u16,
    timestamp: u32,
}

/// Application-level frame packetized by [`Packetizer`].
pub struct PacketizerFrame {
    pub payload: Bytes,
    pub e2ee: Option<E2ee>,
    pub user_timestamp: Option<u64>,
}

impl Packetizer {
    /// Creates a new packetizer.
    pub fn new(track_handle: TrackHandle, mtu_size: usize) -> Self {
        Self { mtu_size, track_handle, sequence: 0, frame_number: 0, timestamp: 0 }
    }

    /// Packetizes a frame into one or more packets.
    pub fn packetize(&mut self, frame: PacketizerFrame) -> impl IntoIterator<Item = Dtp> {
        vec![] // TODO:
    }
}