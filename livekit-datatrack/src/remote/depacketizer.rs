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
    /// Partial frames currently being assembled.
    partials: BTreeMap<u16, PartialFrame>,
}

/// A frame that has been fully reassembled by [`Depacketizer`].
#[derive(Debug)]
pub struct DepacketizerFrame {
    pub payload: Bytes,
    pub extensions: Extensions,
}

impl Depacketizer {
    /// Maximum number of packets to buffer per frame before dropping.
    ///
    /// Large data-track frames can legitimately span well over 128 transport packets
    /// once they are fragmented at the wire MTU, so keep enough headroom to avoid
    /// dropping fully delivered frames during reassembly.
    const MAX_BUFFER_PACKETS: usize = 512;

    /// Maximum number of in-flight frames to buffer before evicting the oldest partial frame.
    const MAX_BUFFER_FRAMES: usize = 64;

    /// Creates a new depacketizer.
    pub fn new() -> Self {
        Self { partials: BTreeMap::new() }
    }

    /// Push a packet into the depacketizer.
    pub fn push(&mut self, packet: Packet) -> DepacketizerPushResult {
        match packet.header.marker {
            FrameMarker::Single => self.frame_from_single(packet),
            FrameMarker::Start | FrameMarker::Inter | FrameMarker::Final => {
                self.push_to_partial(packet)
            }
        }
    }

    fn frame_from_single(&mut self, packet: Packet) -> DepacketizerPushResult {
        debug_assert!(packet.header.marker == FrameMarker::Single);
        let mut result = DepacketizerPushResult::default();
        if let Some(partial) = self.partials.remove(&packet.header.frame_number) {
            result.drop_error = DepacketizerDropError {
                frame_number: partial.frame_number,
                reason: partial.drop_reason(),
            }
            .into();
        }
        result.frame =
            DepacketizerFrame { payload: packet.payload, extensions: packet.header.extensions }
                .into();
        result
    }

    /// Push a non-single packet into the corresponding partial frame.
    fn push_to_partial(&mut self, packet: Packet) -> DepacketizerPushResult {
        let mut result = DepacketizerPushResult::default();
        let frame_number = packet.header.frame_number;

        if !self.partials.contains_key(&frame_number) {
            if let Some(drop) = self.make_room_for_partial() {
                result.drop_error = Some(drop);
            }
            self.partials.insert(
                frame_number,
                PartialFrame::new(frame_number, packet.header.extensions.clone()),
            );
        }

        let Some(partial) = self.partials.get_mut(&frame_number) else {
            return result;
        };
        if partial.payloads.len() >= Self::MAX_BUFFER_PACKETS
            && !partial.payloads.contains_key(&packet.header.sequence)
        {
            self.partials.remove(&frame_number);
            return DepacketizerDropError {
                frame_number,
                reason: DepacketizerDropReason::BufferFull,
            }
            .into();
        }

        partial.push(packet);

        if !partial.is_complete() {
            return result;
        }

        let partial = self.partials.remove(&frame_number).unwrap();
        result.frame = Self::finalize(partial).frame;
        result
    }

    fn make_room_for_partial(&mut self) -> Option<DepacketizerDropError> {
        if self.partials.len() < Self::MAX_BUFFER_FRAMES {
            return None;
        }
        let frame_number = *self.partials.first_key_value()?.0;
        let partial = self.partials.remove(&frame_number)?;
        Some(DepacketizerDropError { frame_number, reason: partial.drop_reason() })
    }

    /// Reassemble a complete frame.
    fn finalize(mut partial: PartialFrame) -> DepacketizerPushResult {
        let start_sequence = partial.start_sequence.expect("complete frame must have start");
        let end_sequence = partial.end_sequence.expect("complete frame must have end");
        let payload_len: usize = partial.payloads.iter().map(|(_, payload)| payload.len()).sum();
        let mut payload = BytesMut::with_capacity(payload_len);

        let mut sequence = start_sequence;

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
        unreachable!("complete frame should contain every packet between start and end")
    }
}

/// Frame being assembled as packets are received.
#[derive(Debug)]
struct PartialFrame {
    /// Frame number from the start packet.
    frame_number: u16,
    /// Sequence of the start packet once it has been observed.
    start_sequence: Option<u16>,
    /// Sequence of the final packet once it has been observed.
    end_sequence: Option<u16>,
    /// Extensions from the start packet.
    extensions: Extensions,
    /// Mapping between sequence number and packet payload.
    payloads: BTreeMap<u16, Bytes>,
}

impl PartialFrame {
    fn new(frame_number: u16, extensions: Extensions) -> Self {
        Self {
            frame_number,
            start_sequence: None,
            end_sequence: None,
            extensions,
            payloads: BTreeMap::default(),
        }
    }

    fn push(&mut self, packet: Packet) {
        if packet.header.marker == FrameMarker::Start {
            self.start_sequence = Some(packet.header.sequence);
            self.extensions = packet.header.extensions.clone();
        }
        if packet.header.marker == FrameMarker::Final {
            self.end_sequence = Some(packet.header.sequence);
            self.extensions = packet.header.extensions.clone();
        }

        if self.payloads.insert(packet.header.sequence, packet.payload).is_some() {
            log::warn!(
                "Duplicate packet for sequence {} on frame {}, replacing with latest",
                packet.header.sequence,
                self.frame_number
            );
        }
    }

    fn is_complete(&self) -> bool {
        let (Some(start_sequence), Some(end_sequence)) = (self.start_sequence, self.end_sequence)
        else {
            return false;
        };

        let expected = end_sequence.wrapping_sub(start_sequence).wrapping_add(1) as usize;
        if self.payloads.len() != expected {
            return false;
        }

        let mut sequence = start_sequence;
        for _ in 0..expected {
            if !self.payloads.contains_key(&sequence) {
                return false;
            }
            sequence = sequence.wrapping_add(1);
        }
        true
    }

    fn drop_reason(&self) -> DepacketizerDropReason {
        match (self.start_sequence, self.end_sequence) {
            (Some(start_sequence), Some(end_sequence)) => DepacketizerDropReason::Incomplete {
                received: self.payloads.len() as u16,
                expected: end_sequence.wrapping_sub(start_sequence).wrapping_add(1),
            },
            (Some(_), None) => DepacketizerDropReason::Interrupted,
            (None, _) => DepacketizerDropReason::UnknownFrame,
        }
    }
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
            packet.header.sequence = packet.header.sequence.wrapping_add(1);

            let result = depacketizer.push(packet.clone());
            assert!(result.frame.is_none() && result.drop_error.is_none());
        }

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence = packet.header.sequence.wrapping_add(1);

        let result = depacketizer.push(packet.clone());

        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();

        assert_eq!(frame.extensions, packet.header.extensions);
        assert_eq!(frame.payload.len(), packet.payload.len() * (inter_packets + 2));
    }

    #[test]
    fn test_buffer_eviction_drops_oldest_partial() {
        let mut depacketizer = Depacketizer::new();

        let mut sequence = Counter::new(0);
        let first_frame_number = 0;
        let mut result = DepacketizerPushResult::default();
        for frame_number in 0..=Depacketizer::MAX_BUFFER_FRAMES as u16 {
            let mut packet: Packet = Faker.fake();
            packet.header.frame_number = frame_number;
            packet.header.marker = FrameMarker::Start;
            packet.header.sequence = sequence.get_then_increment();
            result = depacketizer.push(packet);
        }
        assert!(result.frame.is_none());

        let drop = result.drop_error.unwrap();
        assert_eq!(drop.frame_number, first_frame_number);
        assert!(matches!(drop.reason, DepacketizerDropReason::Interrupted));
    }

    #[test]
    fn test_waits_for_missing_packets() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        let payload_len = packet.payload.len();
        packet.header.marker = FrameMarker::Start;
        packet.header.sequence = 10;

        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        packet.header.sequence = 13;
        packet.header.marker = FrameMarker::Final;

        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        packet.header.sequence = 11;
        packet.header.marker = FrameMarker::Inter;
        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        packet.header.sequence = 12;
        let result = depacketizer.push(packet);
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();
        assert_eq!(frame.payload.len(), 4 * payload_len);
    }

    #[test]
    fn test_waits_for_start_packet() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        let payload_len = packet.payload.len();
        packet.header.sequence = 20;
        packet.header.marker = FrameMarker::Inter;

        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        packet.header.sequence = 19;
        packet.header.marker = FrameMarker::Start;
        let result = depacketizer.push(packet.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        packet.header.sequence = 21;
        packet.header.marker = FrameMarker::Final;
        let result = depacketizer.push(packet);
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();
        assert_eq!(frame.payload.len(), 3 * payload_len);
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
    fn test_interleaved_frames() {
        let mut depacketizer = Depacketizer::new();

        let mut first: Packet = Faker.fake();
        first.header.frame_number = 10;
        first.header.sequence = 100;
        first.header.marker = FrameMarker::Start;
        first.payload = Bytes::from(vec![0xAA; 3]);
        let result = depacketizer.push(first.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        let mut second = first.clone();
        second.header.frame_number = 11;
        second.header.sequence = 102;
        second.header.marker = FrameMarker::Start;
        second.payload = Bytes::from(vec![0xBB; 3]);
        let result = depacketizer.push(second.clone());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        first.header.sequence = 101;
        first.header.marker = FrameMarker::Final;
        let result = depacketizer.push(first);
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();
        assert_eq!(&frame.payload[..3], &[0xAA; 3]);

        second.header.sequence = 103;
        second.header.marker = FrameMarker::Final;
        let result = depacketizer.push(second);
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();
        assert_eq!(&frame.payload[..3], &[0xBB; 3]);
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
