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

use crate::{
    dtp::{
        time::{Clock, Timestamp},
        Dtp, E2ee, Header, TrackHandle,
    },
    utils::{BytesChunkExt, Counter},
};
use bytes::Bytes;
use thiserror::Error;

/// Converts application-level frames into packets for transport.
#[derive(Debug)]
pub struct Packetizer {
    track_handle: TrackHandle,
    mtu_size: usize,
    sequence: Counter<u16>,
    frame_number: Counter<u16>,
    clock: Clock<90_000>,
}

/// Frame packetized by [`Packetizer`].
pub struct PacketizerFrame {
    pub payload: Bytes,
    pub e2ee: Option<E2ee>,
    pub user_timestamp: Option<u64>,
}

#[derive(Error, Debug)]
pub enum PacketizerError {
    #[error("MTU is too short to send frame")]
    MtuTooShort,
}

impl Packetizer {
    /// Creates a new packetizer.
    pub fn new(track_handle: TrackHandle, mtu_size: usize) -> Self {
        Self {
            track_handle,
            mtu_size,
            sequence: Default::default(),
            frame_number: Default::default(),
            clock: Clock::new(Timestamp::random()),
        }
    }

    /// Packetizes a frame into one or more packets.
    pub fn packetize(&mut self, frame: PacketizerFrame) -> Result<Vec<Dtp>, PacketizerError> {
        // TODO: consider using default
        let header = Header {
            is_final: false,
            track_handle: self.track_handle,
            sequence: 0,
            frame_number: self.frame_number.get_then_increment(),
            timestamp: self.clock.now(),
            user_timestamp: frame.user_timestamp,
            e2ee: frame.e2ee,
        };
        let max_payload_size = self.mtu_size.saturating_sub(header.serialized_len());
        if max_payload_size == 0 {
            Err(PacketizerError::MtuTooShort)?
        }

        let packet_payloads: Vec<_> = frame.payload.into_chunks(max_payload_size).collect();
        let packet_count = packet_payloads.len();
        let packets = packet_payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| Dtp {
                header: Header {
                    is_final: index == packet_count - 1,
                    sequence: self.sequence.get_then_increment(),
                    ..header
                },
                payload,
            })
            .collect();
        Ok(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    fn test_packetize(
        #[values(0, 32, 784)] payload_size: usize,
        #[values(256, 1024)] mtu_size: usize,
        #[values(true, false)] with_exts: bool,
    ) {
        let handle = 1u32.try_into().unwrap();
        let e2ee = E2ee { key_index: 255, iv: [0xCD; 12] };
        let user_timestamp = u64::MAX;

        let mut packetizer = Packetizer::new(handle, mtu_size);

        let frame = PacketizerFrame {
            payload: Bytes::from(vec![0xAB; payload_size]),
            e2ee: with_exts.then_some(e2ee),
            user_timestamp: with_exts.then_some(user_timestamp),
        };
        let packets = packetizer.packetize(frame).expect("Failed to packetize");

        if packets.len() == 0 {
            assert_eq!(payload_size, 0, "Should be no packets for zero payload");
            return;
        }

        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet.header.frame_number, 0);
            assert_eq!(packet.header.sequence, index as u16);
            assert_eq!(packet.header.e2ee, with_exts.then_some(e2ee));
            assert_eq!(packet.header.user_timestamp, with_exts.then_some(user_timestamp));
        }
        assert!(packets.last().unwrap().is_final());
    }
}
