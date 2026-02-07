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

use crate::packet::{Extensions, FrameMarker, Packet};
use bytes::{Bytes, BytesMut};
use std::{collections::BTreeMap, fmt::Display};
use thiserror::Error;

/// Reassembles packets into frames.
#[derive(Debug)]
pub struct Depacketizer {
    /// Partial frame currently being assembled.
    partial: Option<PartialFrame>,
}

/// A frame that has been fully reassembled by [`Depacketizer`].
#[derive(Debug)]
pub struct DepacketizerFrame {
    pub payload: Bytes,
    pub extensions: Extensions,
}

impl Depacketizer {
    /// Maximum number of packets to buffer per frame before dropping.
    const MAX_BUFFER_PACKETS: usize = 128;

    /// Creates a new depacketizer.
    pub fn new() -> Self {
        Self { partial: None }
    }

    /// Push a packet into the depacketizer.
    pub fn push(&mut self, packet: Packet) -> DepacketizerPushResult {
        match packet.header.marker {
            FrameMarker::Single => self.frame_from_single(packet),
            FrameMarker::Start => self.begin_partial(packet),
            FrameMarker::Inter | FrameMarker::Final => self.push_to_partial(packet),
        }
    }

    fn frame_from_single(&mut self, packet: Packet) -> DepacketizerPushResult {
        debug_assert!(packet.header.marker == FrameMarker::Single);
        let mut result = DepacketizerPushResult::default();
        if let Some(partial) = self.partial.take() {
            result.drop_error = DepacketizerDropError {
                frame_number: partial.frame_number,
                reason: DepacketizerDropReason::Interrupted,
            }
            .into();
        }
        result.frame =
            DepacketizerFrame { payload: packet.payload, extensions: packet.header.extensions }
                .into();
        result
    }

    /// Begin assembling a new packet.
    fn begin_partial(&mut self, packet: Packet) -> DepacketizerPushResult {
        debug_assert!(packet.header.marker == FrameMarker::Start);

        let mut result = DepacketizerPushResult::default();

        if let Some(partial) = self.partial.take() {
            result.drop_error = DepacketizerDropError {
                frame_number: partial.frame_number,
                reason: DepacketizerDropReason::Interrupted,
            }
            .into();
        }

        let start_sequence = packet.header.sequence;
        let partial = PartialFrame {
            frame_number: packet.header.frame_number,
            start_sequence,
            extensions: packet.header.extensions,
            payloads: BTreeMap::from([(start_sequence, packet.payload)])
        };
        self.partial = partial.into();

        result
    }

    /// Push to the existing partial frame.
    fn push_to_partial(&mut self, packet: Packet) -> DepacketizerPushResult {
        debug_assert!(matches!(packet.header.marker, FrameMarker::Inter | FrameMarker::Final));

        let Some(mut partial) = self.partial.take() else {
            return DepacketizerDropError {
                frame_number: packet.header.frame_number,
                reason: DepacketizerDropReason::UnknownFrame,
            }
            .into();
        };
        if packet.header.frame_number != partial.frame_number {
            return DepacketizerDropError {
                frame_number: partial.frame_number,
                reason: DepacketizerDropReason::Interrupted,
            }
            .into();
        }
        if partial.payloads.len() >= Self::MAX_BUFFER_PACKETS {
            return DepacketizerDropError {
                frame_number: partial.frame_number,
                reason: DepacketizerDropReason::BufferFull,
            }
            .into();
        }

        if partial.payloads.insert(packet.header.sequence, packet.payload).is_some() {
            log::warn!(
                "Duplicate packet for sequence {} on frame {}, using latest",
                packet.header.sequence,
                partial.frame_number
            );
        }

        if packet.header.marker == FrameMarker::Final {
            return Self::finalize(partial, packet.header.sequence);
        }

        self.partial = Some(partial);
        DepacketizerPushResult::default()
    }

    /// Try to reassemble the complete frame.
    fn finalize(mut partial: PartialFrame, end_sequence: u16) -> DepacketizerPushResult {
        let received = partial.payloads.len() as u16;

        let payload_len: usize = partial.payloads.iter().map(|(_, payload)| payload.len()).sum();
        let mut payload = BytesMut::with_capacity(payload_len);

        let mut sequence = partial.start_sequence;

        while let Some(partial_payload) = partial.payloads.remove(&sequence) {
            debug_assert!(payload.len() + partial_payload.len() <= payload.capacity());
            payload.extend(partial_payload);

            if sequence != end_sequence {
                sequence = sequence.wrapping_add(1);
                continue;
            }
            return DepacketizerFrame { payload: payload.freeze(), extensions: partial.extensions }
                .into();
        }
        DepacketizerDropError {
            frame_number: partial.frame_number,
            reason: DepacketizerDropReason::Incomplete {
                received,
                expected: end_sequence.wrapping_sub(partial.start_sequence).wrapping_add(1),
            },
        }
        .into()
    }
}

/// Frame being assembled as packets are received.
#[derive(Debug)]
struct PartialFrame {
    /// Frame number from the start packet.
    frame_number: u16,
    /// Sequence of the start packet.
    start_sequence: u16,
    /// Extensions from the start packet.
    extensions: Extensions,
    /// Mapping between sequence number and packet payload.
    payloads: BTreeMap<u16, Bytes>,
}

/// Result from a call to [`Depacketizer::push`].
///
/// The reason this type is used instead of [`core::result::Result`] is due to the fact a single
/// call to push can result in both a complete frame being delivered and a previous
/// frame being dropped.
///
#[derive(Debug, Default)]
pub struct DepacketizerPushResult {
    pub frame: Option<DepacketizerFrame>,
    pub drop_error: Option<DepacketizerDropError>,
}

impl From<DepacketizerFrame> for DepacketizerPushResult {
    fn from(frame: DepacketizerFrame) -> Self {
        Self { frame: frame.into(), ..Default::default() }
    }
}

impl From<DepacketizerDropError> for DepacketizerPushResult {
    fn from(drop_event: DepacketizerDropError) -> Self {
        Self { drop_error: drop_event.into(), ..Default::default() }
    }
}

/// An error indicating a frame was dropped.
#[derive(Debug, Error)]
#[error("Frame {frame_number} dropped: {reason}")]
pub struct DepacketizerDropError {
    frame_number: u16,
    reason: DepacketizerDropReason,
}

/// Reason why a frame was dropped.
#[derive(Debug)]
pub enum DepacketizerDropReason {
    /// Interrupted by the start of a new frame.
    Interrupted,
    /// Initial packet was never received.
    UnknownFrame,
    /// Reorder buffer is full.
    BufferFull,
    /// Not all packets received before final packet.
    Incomplete {
        /// Number of packets received.
        received: u16,
        /// Number of packets expected.
        expected: u16,
    },
}

impl Display for DepacketizerDropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepacketizerDropReason::Interrupted => write!(f, "interrupted"),
            DepacketizerDropReason::UnknownFrame => write!(f, "unknown frame"),
            DepacketizerDropReason::BufferFull => write!(f, "buffer full"),
            DepacketizerDropReason::Incomplete { received, expected } => {
                write!(f, "incomplete ({}/{})", received, expected)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::Counter;
    use fake::{Fake, Faker};
    use test_case::test_case;

    #[test]
    fn test_single_packet() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Single;

        let result = depacketizer.push(packet.clone());

        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();

        assert_eq!(frame.payload, packet.payload);
        assert_eq!(frame.extensions, packet.header.extensions);
    }

    #[test_case(0)]
    #[test_case(8)]
    #[test_case(Depacketizer::MAX_BUFFER_PACKETS - 2 ; "buffer_limit")]
    fn test_multi_packet(inter_packets: usize) {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Start;

        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        for _ in 0..inter_packets {
            packet.header.marker = FrameMarker::Inter;
            packet.header.sequence += 1;

            let result = depacketizer.push(packet.clone());
            assert!(result.frame.is_none() && result.drop_error.is_none());
        }

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence += 1;

        let result = depacketizer.push(packet.clone());

        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();

        assert_eq!(frame.extensions, packet.header.extensions);
        assert_eq!(frame.payload.len(), packet.payload.len() * (inter_packets + 2));
    }

    #[test]
    fn test_interrupted() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Start;

        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        let first_frame_number = packet.header.frame_number;
        packet.header.frame_number += 1; // Next frame

        let result = depacketizer.push(packet);
        assert!(result.frame.is_none());

        let drop = result.drop_error.unwrap();
        assert_eq!(drop.frame_number, first_frame_number);
        assert!(matches!(drop.reason, DepacketizerDropReason::Interrupted));
    }

    #[test]
    fn test_incomplete() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        let frame_number = packet.header.frame_number;
        packet.header.marker = FrameMarker::Start;

        depacketizer.push(packet.clone());

        packet.header.sequence += 3;
        packet.header.marker = FrameMarker::Final;

        let result = depacketizer.push(packet);
        assert!(result.frame.is_none());

        let drop = result.drop_error.unwrap();
        assert_eq!(drop.frame_number, frame_number);
        assert!(matches!(
            drop.reason,
            DepacketizerDropReason::Incomplete { received: 2, expected: 4 }
        ));
    }

    #[test]
    fn test_unknown_frame() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        let frame_number = packet.header.frame_number;
        packet.header.marker = FrameMarker::Inter;
        // Start packet for this frame will never be pushed.

        let result = depacketizer.push(packet);
        let drop = result.drop_error.unwrap();
        assert_eq!(drop.frame_number, frame_number);
        assert!(matches!(drop.reason, DepacketizerDropReason::UnknownFrame));
    }

    #[test]
    fn test_multi_frame() {
        let mut depacketizer = Depacketizer::new();

        let mut sequence = Counter::new(0);
        for frame_number in 0..10 {
            let mut packet: Packet = Faker.fake();
            packet.header.frame_number = frame_number;
            packet.header.marker = FrameMarker::Start;
            packet.header.sequence = sequence.get_then_increment();

            let result = depacketizer.push(packet.clone());
            assert!(result.drop_error.is_none() && result.frame.is_none());

            packet.header.marker = FrameMarker::Inter;
            packet.header.sequence = sequence.get_then_increment();

            let result = depacketizer.push(packet.clone());
            assert!(result.drop_error.is_none() && result.frame.is_none());

            packet.header.marker = FrameMarker::Final;
            packet.header.sequence = sequence.get_then_increment();

            let result = depacketizer.push(packet);
            assert!(result.drop_error.is_none() && result.frame.is_some());
        }
    }

    #[test]
    fn test_duplicate_sequence_numbers() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Start;
        packet.header.sequence = 1;
        packet.payload = Bytes::from(vec![0xAB; 3]);

        let result = depacketizer.push(packet.clone());
        assert!(result.drop_error.is_none() && result.frame.is_none());

        packet.header.marker = FrameMarker::Inter;
        packet.header.sequence = 1; // Same sequence number
        packet.payload = Bytes::from(vec![0xCD; 3]);

        let result = depacketizer.push(packet.clone());
        assert!(result.drop_error.is_none() && result.frame.is_none());

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence = 2;
        packet.payload = Bytes::from(vec![0xEF; 3]);

        let result = depacketizer.push(packet.clone());
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();

        assert!(frame.payload.starts_with(&[0xCD; 3]));
        // Should retain the second packet with duplicate sequence number
    }

    impl fake::Dummy<fake::Faker> for Packet {
        fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
            let payload_len = rng.random_range(0..=1500);
            let payload = (0..payload_len).map(|_| rng.random()).collect::<Bytes>();
            Self { header: Faker.fake_with_rng(rng), payload }
        }
    }
}
