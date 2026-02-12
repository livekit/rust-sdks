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
    packet::{Clock, Extensions, FrameMarker, Handle, Header, Packet, Timestamp},
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
    pub fn packetize(&mut self, frame: PacketizerFrame) -> Result<Vec<Packet>, PacketizerError> {
        let mut header = Header {
            marker: FrameMarker::Inter,
            track_handle: self.handle,
            sequence: 0,
            frame_number: 0,
            timestamp: self.clock.now(),
            extensions: frame.extensions,
        };
        let max_payload_size = self.mtu_size.saturating_sub(header.serialized_len());
        if max_payload_size == 0 {
            Err(PacketizerError::MtuTooShort)?
        }
        header.frame_number = self.frame_number.get_then_increment();

        let packet_payloads: Vec<_> = frame.payload.into_chunks(max_payload_size).collect();
        let packet_count = packet_payloads.len();
        let packets = packet_payloads
            .into_iter()
            .enumerate()
            .map(|(index, payload)| Packet {
                header: Header {
                    marker: Self::frame_marker(index, packet_count),
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
        if packet_count <= 1 {
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
    use crate::packet::Handle;
    use fake::{Fake, Faker};
    use test_case::test_case;

    #[test_case(0, 1, FrameMarker::Single)]
    #[test_case(0, 10, FrameMarker::Start)]
    #[test_case(4, 10, FrameMarker::Inter)]
    #[test_case(9, 10, FrameMarker::Final)]
    fn test_frame_marker(index: usize, packet_count: usize, expected_marker: FrameMarker) {
        assert_eq!(Packetizer::frame_marker(index, packet_count), expected_marker);
    }

    #[test_case(0, 1_024 ; "zero_payload")]
    #[test_case(128, 1_024 ; "single_packet")]
    #[test_case(20_480, 1_024 ; "multi_packet")]
    #[test_case(40_960, 16_000 ; "multi_packet_mtu_16000")]
    fn test_packetize(payload_size: usize, mtu_size: usize) {
        let handle: Handle = Faker.fake();
        let extensions: Extensions = Faker.fake();

        let mut packetizer = Packetizer::new(handle, mtu_size);

        let frame = PacketizerFrame {
            payload: Bytes::from(vec![0xAB; payload_size]),
            extensions: extensions.clone(),
        };
        let packets = packetizer.packetize(frame).expect("Failed to packetize");

        if packets.len() == 0 {
            assert_eq!(payload_size, 0, "Should be no packets for zero payload");
            return;
        }

        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet.header.marker, Packetizer::frame_marker(index, packets.len()));
            assert_eq!(packet.header.frame_number, 0);
            assert_eq!(packet.header.track_handle, handle);
            assert_eq!(packet.header.sequence, index as u16);
            assert_eq!(packet.header.extensions, extensions);
        }
    }
}
