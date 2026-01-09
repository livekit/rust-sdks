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
    dtp::{Clock, Dtp, Extensions, FrameMarker, Handle, Header, Timestamp},
    utils::{BytesChunkExt, Counter},
};
use bytes::Bytes;
use thiserror::Error;

/// Converts application-level frames into packets for transport.
#[derive(Debug)]
pub struct Packetizer {
    handle: Handle,
    mtu_size: usize,
    sequence: Counter<u16>,
    frame_number: Counter<u16>,
    clock: Clock<90_000>,
}

/// Frame packetized by [`Packetizer`].
pub struct PacketizerFrame {
    pub payload: Bytes,
    pub extensions: Extensions,
}

#[derive(Error, Debug)]
pub enum PacketizerError {
    #[error("MTU is too short to send frame")]
    MtuTooShort,
}

impl Packetizer {
    /// Creates a new packetizer.
    pub fn new(track_handle: Handle, mtu_size: usize) -> Self {
        Self {
            handle: track_handle,
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
            frame_marker: FrameMarker::Inter,
            track_handle: self.handle,
            sequence: 0,
            frame_number: self.frame_number.get_then_increment(),
            timestamp: self.clock.now(),
            extensions: frame.extensions,
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
                    frame_marker: Self::frame_marker(index, packet_count),
                    sequence: self.sequence.get_then_increment(),
                    extensions: header.extensions.clone(),
                    ..header
                },
                payload,
            })
            .collect();
        Ok(packets)
    }

    fn frame_marker(index: usize, packet_count: usize) -> FrameMarker {
        if packet_count == 1 {
            return FrameMarker::Single;
        }
        match index {
            0 => FrameMarker::Start,
            _ if index == packet_count - 1 => FrameMarker::Final,
            _ => FrameMarker::Inter,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dtp::{E2eeExt, UserTimestampExt};
    use rstest::rstest;

    #[rstest]
    fn test_packetize(
        #[values(0, 32, 784)] payload_size: usize,
        #[values(256, 1024)] mtu_size: usize,
        #[values(true, false)] with_exts: bool,
    ) {
        let handle = 1u32.try_into().unwrap();
        let e2ee = E2eeExt { key_index: 255, iv: [0xCD; 12] };
        let user_timestamp = UserTimestampExt(u64::MAX);

        let mut packetizer = Packetizer::new(handle, mtu_size);

        let frame = PacketizerFrame {
            payload: Bytes::from(vec![0xAB; payload_size]),
            extensions: Extensions {
                e2ee: with_exts.then_some(e2ee),
                user_timestamp: with_exts.then_some(user_timestamp),
            },
        };
        let packets = packetizer.packetize(frame).expect("Failed to packetize");

        if packets.len() == 0 {
            assert_eq!(payload_size, 0, "Should be no packets for zero payload");
            return;
        }

        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet.header.frame_number, 0);
            assert_eq!(packet.header.sequence, index as u16);
            assert_eq!(packet.header.extensions.e2ee, with_exts.then_some(e2ee));
            assert_eq!(
                packet.header.extensions.user_timestamp,
                with_exts.then_some(user_timestamp)
            );
        }
        assert_eq!(packets.last().unwrap().header.frame_marker, FrameMarker::Final);
    }
}
