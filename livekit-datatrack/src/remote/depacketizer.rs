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
use indexmap::IndexMap;
use std::{collections::BTreeMap, fmt::Display};
use thiserror::Error;

/// Reassembles packets into frames.
#[derive(Debug)]
pub(super) struct Depacketizer {
    /// Partial frames currently being assembled, keyed by frame number.
    ///
    /// `IndexMap` preserves insertion order, so the oldest entry is the first key.
    ///
    partials: IndexMap<u16, PartialFrame>,
}

/// A frame that has been fully reassembled by [`Depacketizer`].
#[derive(Debug)]
pub(super) struct DepacketizerFrame {
    pub payload: Bytes,
    pub extensions: Extensions,
}

/// Options accepted by [`Depacketizer::push`].
#[derive(Debug, Clone, Copy)]
pub(super) struct DepacketizerPushOptions {
    /// Maximum number of partial frames the depacketizer will track concurrently. When a new
    /// frame arrives while the partials map is at capacity, the oldest partial is evicted.
    pub max_partial_frames: usize,
}

impl Default for DepacketizerPushOptions {
    fn default() -> Self {
        Self { max_partial_frames: 1 }
    }
}

impl Depacketizer {
    /// Maximum number of packets to buffer per frame before dropping.
    const MAX_BUFFER_PACKETS: usize = 128;

    /// Creates a new depacketizer.
    pub fn new() -> Self {
        Self { partials: IndexMap::new() }
    }

    /// Push a packet into the depacketizer.
    pub fn push(
        &mut self,
        packet: Packet,
        options: DepacketizerPushOptions,
    ) -> DepacketizerPushResult {
        match packet.header.marker {
            FrameMarker::Single => self.frame_from_single(packet, options),
            FrameMarker::Start => self.begin_partial(packet, options),
            FrameMarker::Inter | FrameMarker::Final => self.push_to_partial(packet),
        }
    }

    fn frame_from_single(
        &mut self,
        packet: Packet,
        options: DepacketizerPushOptions,
    ) -> DepacketizerPushResult {
        debug_assert!(packet.header.marker == FrameMarker::Single);
        let mut result = DepacketizerPushResult::default();

        // A `Single` packet is a self-contained frame and does not occupy a partials slot,
        // but if the partials map is at capacity it is treated as a signal that the oldest
        // in-flight partial is stale and evict it.
        if self.partials.len() >= options.max_partial_frames {
            result.drop_error = self.evict_oldest(packet.header.frame_number);
        }

        result.frame =
            DepacketizerFrame { payload: packet.payload, extensions: packet.header.extensions }
                .into();
        result
    }

    /// Evicts the oldest partial (by insertion order) and returns the drop error describing
    /// it, attributing the interruption to `new_frame_number`. Returns `None` when there are
    /// no partials to evict.
    fn evict_oldest(&mut self, new_frame_number: u16) -> Option<DepacketizerDropError> {
        let (&oldest, _) = self.partials.first()?;
        self.partials.shift_remove(&oldest);
        DepacketizerDropError {
            frame_number: oldest,
            reason: DepacketizerDropReason::Interrupted { new_frame_number },
        }
        .into()
    }

    /// Begin assembling a new partial frame.
    fn begin_partial(
        &mut self,
        packet: Packet,
        options: DepacketizerPushOptions,
    ) -> DepacketizerPushResult {
        debug_assert!(packet.header.marker == FrameMarker::Start);

        let mut result = DepacketizerPushResult::default();
        let frame_number = packet.header.frame_number;

        // Loop in case `max_partial_frames` shrunk relative to a previous push call - evict
        // the oldest partials until there is room for the new one. Only the first eviction
        // is surfaced in `drop_error`; the rest are silently dropped to keep the public
        // `DepacketizerPushResult` shape unchanged.
        while self.partials.len() >= options.max_partial_frames {
            let Some(evicted) = self.evict_oldest(frame_number) else {
                // Partials map is empty - nothing more to evict.
                break;
            };
            if result.drop_error.is_none() {
                result.drop_error = Some(evicted);
            }
        }

        let start_sequence = packet.header.sequence;
        let partial = PartialFrame {
            start_sequence,
            extensions: packet.header.extensions,
            payloads: BTreeMap::from([(start_sequence, packet.payload)]),
        };
        self.partials.insert(frame_number, partial);

        result
    }

    /// Push to the partial frame matching the packet's frame number.
    fn push_to_partial(&mut self, packet: Packet) -> DepacketizerPushResult {
        debug_assert!(matches!(packet.header.marker, FrameMarker::Inter | FrameMarker::Final));

        let frame_number = packet.header.frame_number;
        let Some(partial) = self.partials.get_mut(&frame_number) else {
            return DepacketizerDropError {
                frame_number,
                reason: DepacketizerDropReason::UnknownFrame,
            }
            .into();
        };

        if partial.payloads.len() >= Self::MAX_BUFFER_PACKETS {
            self.partials.shift_remove(&frame_number);
            return DepacketizerDropError {
                frame_number,
                reason: DepacketizerDropReason::BufferFull,
            }
            .into();
        }

        if partial.payloads.insert(packet.header.sequence, packet.payload).is_some() {
            log::warn!(
                "Duplicate packet for sequence {} on frame {}, replacing with latest",
                packet.header.sequence,
                frame_number
            );
        }

        if packet.header.marker == FrameMarker::Final {
            let partial = self.partials.shift_remove(&frame_number).expect("partial just modified");
            return Self::finalize(frame_number, partial, packet.header.sequence);
        }

        DepacketizerPushResult::default()
    }

    /// Try to reassemble the complete frame.
    fn finalize(
        frame_number: u16,
        mut partial: PartialFrame,
        end_sequence: u16,
    ) -> DepacketizerPushResult {
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
            frame_number,
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
pub(super) struct DepacketizerPushResult {
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
#[derive(Debug, Error, PartialEq)]
#[error("Frame {frame_number} dropped: {reason}")]
pub(super) struct DepacketizerDropError {
    pub(super) frame_number: u16,
    pub(super) reason: DepacketizerDropReason,
}

/// Reason why a frame was dropped.
#[derive(Debug, PartialEq)]
pub(super) enum DepacketizerDropReason {
    /// Interrupted by the start of a new frame.
    Interrupted {
        /// Frame number of the new frame that triggered the eviction.
        new_frame_number: u16,
    },
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
            DepacketizerDropReason::Interrupted { new_frame_number } => {
                write!(f, "interrupted by new frame {}", new_frame_number)
            }
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
    use crate::{packet::Header, utils::Counter};
    use fake::{Fake, Faker};
    use test_case::test_case;

    #[test]
    fn test_single_packet() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Single;

        let result = depacketizer.push(packet.clone(), Default::default());

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

        let result = depacketizer.push(packet.clone(), Default::default());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        for _ in 0..inter_packets {
            packet.header.marker = FrameMarker::Inter;
            packet.header.sequence = packet.header.sequence.wrapping_add(1);

            let result = depacketizer.push(packet.clone(), Default::default());
            assert!(result.frame.is_none() && result.drop_error.is_none());
        }

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence = packet.header.sequence.wrapping_add(1);

        let result = depacketizer.push(packet.clone(), Default::default());

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

        let result = depacketizer.push(packet.clone(), Default::default());
        assert!(result.frame.is_none() && result.drop_error.is_none());

        let first_frame_number = packet.header.frame_number;
        let new_frame_number = packet.header.frame_number.wrapping_add(1);
        packet.header.frame_number = new_frame_number; // Next frame

        let result = depacketizer.push(packet, Default::default());
        assert!(result.frame.is_none());

        let drop = result.drop_error.unwrap();
        assert_eq!(drop.frame_number, first_frame_number);
        let DepacketizerDropReason::Interrupted { new_frame_number: reported } = drop.reason else {
            panic!("Expected Interrupted, got {:?}", drop.reason);
        };
        assert_eq!(reported, first_frame_number.wrapping_add(1));
    }

    #[test]
    fn test_incomplete() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        let frame_number = packet.header.frame_number;
        packet.header.marker = FrameMarker::Start;

        depacketizer.push(packet.clone(), Default::default());

        packet.header.sequence = packet.header.sequence.wrapping_add(3);
        packet.header.marker = FrameMarker::Final;

        let result = depacketizer.push(packet, Default::default());
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

        let result = depacketizer.push(packet, Default::default());
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

            let result = depacketizer.push(packet.clone(), Default::default());
            assert!(result.drop_error.is_none() && result.frame.is_none());

            packet.header.marker = FrameMarker::Inter;
            packet.header.sequence = sequence.get_then_increment();

            let result = depacketizer.push(packet.clone(), Default::default());
            assert!(result.drop_error.is_none() && result.frame.is_none());

            packet.header.marker = FrameMarker::Final;
            packet.header.sequence = sequence.get_then_increment();

            let result = depacketizer.push(packet, Default::default());
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

        let result = depacketizer.push(packet.clone(), Default::default());
        assert!(result.drop_error.is_none() && result.frame.is_none());

        packet.header.marker = FrameMarker::Inter;
        packet.header.sequence = 1; // Same sequence number
        packet.payload = Bytes::from(vec![0xCD; 3]);

        let result = depacketizer.push(packet.clone(), Default::default());
        assert!(result.drop_error.is_none() && result.frame.is_none());

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence = 2;
        packet.payload = Bytes::from(vec![0xEF; 3]);

        let result = depacketizer.push(packet.clone(), Default::default());
        assert!(result.drop_error.is_none());
        let frame = result.frame.unwrap();

        assert!(frame.payload.starts_with(&[0xCD; 3]));
        // Should retain the second packet with duplicate sequence number
    }

    /// Should assemble multiple partial frames concurrently when
    /// `max_partial_frames` is set.
    #[test]
    fn test_assembles_multiple_partial_frames() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 2 };

        let base: Packet = Faker.fake();
        let payload_len = base.payload.len();

        // Begin frame A
        let start_a = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 0,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_a, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        // Begin frame B - should not give error because we're under capacity
        let start_b = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 100,
                frame_number: 2,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_b, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        // Complete frame A out of order - should produce a frame
        let final_a = Packet {
            header: Header {
                marker: FrameMarker::Final,
                sequence: 1,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(final_a, opts);
        assert!(result.drop_error.is_none());
        assert_eq!(result.frame.expect("Expected frame").payload.len(), payload_len * 2);

        // Frame B is still in flight and should still complete cleanly
        let final_b = Packet {
            header: Header {
                marker: FrameMarker::Final,
                sequence: 101,
                frame_number: 2,
                ..base.header
            },
            ..base
        };
        let result = depacketizer.push(final_b, opts);
        assert!(result.drop_error.is_none());
        assert_eq!(result.frame.expect("Expected frame").payload.len(), payload_len * 2);
    }

    /// Should report a drop when starting a new partial frame
    /// would exceed `max_partial_frames`.
    #[test]
    fn test_starting_new_frame_at_capacity() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 2 };

        let base: Packet = Faker.fake();

        // Fill the partials map with two in-flight frames
        let start_a = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 0,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_a, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        let start_b = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 100,
                frame_number: 2,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_b, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        // A third in-flight start should throw, naming the oldest evicted frame (1) and the new one (3)
        let start_c = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 200,
                frame_number: 3,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_c, opts);

        let expected_error = DepacketizerDropError {
            frame_number: 1,
            reason: DepacketizerDropReason::Interrupted { new_frame_number: 3 },
        };
        assert!(result.frame.is_none());
        assert_eq!(result.drop_error, Some(expected_error));
    }

    /// Should report a drop when a single-packet frame arrives while the
    /// partials map is at capacity.
    #[test]
    fn test_single_packet_at_capacity() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 2 };

        let base: Packet = Faker.fake();

        // Fill the partials map with two in-flight frames
        let start_a = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 0,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_a, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        let start_b = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 100,
                frame_number: 2,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(start_b, opts);
        assert!(result.frame.is_none() && result.drop_error.is_none());

        // A single-packet frame arriving at capacity should evict the oldest (frame 1) and gives error
        let single_c = Packet {
            header: Header {
                marker: FrameMarker::Single,
                sequence: 200,
                frame_number: 3,
                ..base.header
            },
            ..base
        };
        let result = depacketizer.push(single_c, opts);

        let expected_error = DepacketizerDropError {
            frame_number: 1,
            reason: DepacketizerDropReason::Interrupted { new_frame_number: 3 },
        };
        assert_eq!(result.drop_error, Some(expected_error));
    }

    /// Should evict the oldest partial frame when start packets
    /// exceed `max_partial_frames`.
    #[test]
    fn test_evicts_oldest_when_starts_exceed_max() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 5 };

        let total_frames: u16 = 10;
        let base: Packet = Faker.fake();

        // Begin 10 partial frames. Each frame's Start uses sequence i*2; its Final uses
        // i*2 + 1. After all 10 starts, only frames 6..10 remain in the partials map
        // (oldest evicted first).
        for i in 0..total_frames {
            let start = Packet {
                header: Header {
                    marker: FrameMarker::Start,
                    sequence: i * 2,
                    frame_number: i + 1,
                    ..base.header.clone()
                },
                ..base.clone()
            };
            assert!(depacketizer.push(start, opts).frame.is_none());
        }

        // Send Final for each frame. Frames 1..5 were evicted → unknownFrame; frames 6..10 produce.
        let mut produced_frames = 0;
        let mut unknown_frame_errors = 0;
        for i in 0..total_frames {
            let final_packet = Packet {
                header: Header {
                    marker: FrameMarker::Final,
                    sequence: i * 2 + 1,
                    frame_number: i + 1,
                    ..base.header.clone()
                },
                ..base.clone()
            };
            let result = depacketizer.push(final_packet, opts);
            if result.frame.is_some() {
                produced_frames += 1;
            }
            if let Some(drop) = result.drop_error {
                assert_eq!(drop.reason, DepacketizerDropReason::UnknownFrame);
                unknown_frame_errors += 1;
            }
        }

        assert_eq!(produced_frames, 5);
        assert_eq!(unknown_frame_errors, 5);
    }

    /// Should report `UnknownFrame` for late inter and final packets
    /// belonging to an evicted frame.
    #[test]
    fn test_late_packets_for_evicted_frame() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 3 };

        let base: Packet = Faker.fake();

        // Fill the partials map with three in-flight frames.
        for i in 1u16..=3 {
            let start = Packet {
                header: Header {
                    marker: FrameMarker::Start,
                    sequence: i * 100,
                    frame_number: i,
                    ..base.header.clone()
                },
                ..base.clone()
            };
            assert!(depacketizer.push(start, opts).frame.is_none());
        }

        // A fourth Start evicts the oldest (frame 1).
        let start_four = Packet {
            header: Header {
                marker: FrameMarker::Start,
                sequence: 400,
                frame_number: 4,
                ..base.header.clone()
            },
            ..base.clone()
        };
        assert!(depacketizer.push(start_four, opts).frame.is_none());

        // A late Inter for the evicted frame 1 should report UnknownFrame.
        let late_inter_one = Packet {
            header: Header {
                marker: FrameMarker::Inter,
                sequence: 101,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(late_inter_one, opts);
        let expected_error =
            DepacketizerDropError { frame_number: 1, reason: DepacketizerDropReason::UnknownFrame };
        assert!(result.frame.is_none());
        assert_eq!(result.drop_error, Some(expected_error));

        // A late Final for the evicted frame 1 should also report UnknownFrame.
        let late_final_one = Packet {
            header: Header {
                marker: FrameMarker::Final,
                sequence: 102,
                frame_number: 1,
                ..base.header.clone()
            },
            ..base.clone()
        };
        let result = depacketizer.push(late_final_one, opts);
        let expected_error =
            DepacketizerDropError { frame_number: 1, reason: DepacketizerDropReason::UnknownFrame };
        assert!(result.frame.is_none());
        assert_eq!(result.drop_error, Some(expected_error));

        // Frames 2, 3 and 4 should all still complete cleanly despite the late packets for frame 1.
        for frame_number in [2u16, 3, 4] {
            let final_packet = Packet {
                header: Header {
                    marker: FrameMarker::Final,
                    sequence: frame_number * 100 + 1,
                    frame_number,
                    ..base.header.clone()
                },
                ..base.clone()
            };
            assert!(depacketizer.push(final_packet, opts).frame.is_some());
        }
    }

    /// Should keep partial frame state isolated when packets for multiple
    /// frames are heavily interleaved.
    #[test]
    fn test_heavily_interleaved_frames() {
        let mut depacketizer = Depacketizer::new();
        let opts = DepacketizerPushOptions { max_partial_frames: 3 };

        let base: Packet = Faker.fake();

        // Three frames each carrying three uniquely-tagged payloads. Sequence ranges are
        // chosen so that no two frames share a sequence value.
        struct FrameSpec {
            frame_number: u16,
            start_sequence: u16,
            payloads: [&'static [u8]; 3],
        }
        let frames = [
            FrameSpec { frame_number: 1, start_sequence: 0, payloads: [&[0xa1], &[0xa2], &[0xa3]] },
            FrameSpec {
                frame_number: 2,
                start_sequence: 100,
                payloads: [&[0xb1], &[0xb2], &[0xb3]],
            },
            FrameSpec {
                frame_number: 3,
                start_sequence: 200,
                payloads: [&[0xc1], &[0xc2], &[0xc3]],
            },
        ];

        let build = |frame_idx: usize, packet_idx: u16, marker: FrameMarker| -> Packet {
            Packet {
                header: Header {
                    marker,
                    sequence: frames[frame_idx].start_sequence + packet_idx,
                    frame_number: frames[frame_idx].frame_number,
                    ..base.header.clone()
                },
                payload: Bytes::from_static(frames[frame_idx].payloads[packet_idx as usize]),
            }
        };

        // Round-robin Starts and Inters across all three frames.
        assert!(depacketizer.push(build(0, 0, FrameMarker::Start), opts).frame.is_none());
        assert!(depacketizer.push(build(1, 0, FrameMarker::Start), opts).frame.is_none());
        assert!(depacketizer.push(build(2, 0, FrameMarker::Start), opts).frame.is_none());
        assert!(depacketizer.push(build(0, 1, FrameMarker::Inter), opts).frame.is_none());
        assert!(depacketizer.push(build(1, 1, FrameMarker::Inter), opts).frame.is_none());
        assert!(depacketizer.push(build(2, 1, FrameMarker::Inter), opts).frame.is_none());

        // Finals arrive in a different order than the Starts to confirm per-frame isolation.
        let frame_two = depacketizer.push(build(1, 2, FrameMarker::Final), opts).frame.unwrap();
        assert_eq!(frame_two.payload.as_ref(), &[0xb1, 0xb2, 0xb3]);

        let frame_one = depacketizer.push(build(0, 2, FrameMarker::Final), opts).frame.unwrap();
        assert_eq!(frame_one.payload.as_ref(), &[0xa1, 0xa2, 0xa3]);

        let frame_three = depacketizer.push(build(2, 2, FrameMarker::Final), opts).frame.unwrap();
        assert_eq!(frame_three.payload.as_ref(), &[0xc1, 0xc2, 0xc3]);
    }

    /// Should respect maxPartialFrames changing across push calls, both expanding
    /// to allow more in-flight frames and shrinking to evict older ones.
    #[test]
    fn test_max_partial_frames_change_across_pushes() {
        let mut depacketizer = Depacketizer::new();
        let mut opts = DepacketizerPushOptions { max_partial_frames: 2 };

        let base: Packet = Faker.fake();

        let start_for = |frame_number: u16| -> Packet {
            Packet {
                header: Header {
                    marker: FrameMarker::Start,
                    sequence: frame_number * 100,
                    frame_number,
                    ..base.header.clone()
                },
                ..base.clone()
            }
        };
        let final_for = |frame_number: u16| -> Packet {
            Packet {
                header: Header {
                    marker: FrameMarker::Final,
                    sequence: frame_number * 100 + 1,
                    frame_number,
                    ..base.header.clone()
                },
                ..base.clone()
            }
        };

        // Fill the partials map exactly with max_partial_frames=2.
        assert!(depacketizer.push(start_for(1), opts).frame.is_none());
        assert!(depacketizer.push(start_for(2), opts).frame.is_none());

        // Expand max_partial_frames to 4 mid-stream. Frames 3 and 4 should be added without
        // evicting anything; assert no interruption fires.
        opts.max_partial_frames = 4;

        let result = depacketizer.push(start_for(3), opts);
        assert!(result.frame.is_none());
        assert_eq!(result.drop_error, None);

        let result = depacketizer.push(start_for(4), opts);
        assert!(result.frame.is_none());
        assert_eq!(result.drop_error, None);

        // Spot-check that frame 1 is still tracked despite the cap changes.
        assert!(depacketizer.push(final_for(1), opts).frame.is_some());
        // Three partials remain in flight: frames 2, 3, 4.

        // Shrink max_partial_frames to 2. Adding frame 5 should evict frames 2 and 3 in this
        // single push call to bring the in-flight count back under the new cap. Only the
        // first eviction is surfaced via drop_error; the second is silently dropped.
        opts.max_partial_frames = 2;

        assert!(depacketizer.push(start_for(5), opts).frame.is_none());

        // Only frames 4 and 5 should remain in the map.
        let result = depacketizer.push(final_for(2), opts);
        let expected_error =
            DepacketizerDropError { frame_number: 2, reason: DepacketizerDropReason::UnknownFrame };
        assert_eq!(result.drop_error, Some(expected_error));

        let result = depacketizer.push(final_for(3), opts);
        let expected_error =
            DepacketizerDropError { frame_number: 3, reason: DepacketizerDropReason::UnknownFrame };
        assert_eq!(result.drop_error, Some(expected_error));

        assert!(depacketizer.push(final_for(4), opts).frame.is_some());
        assert!(depacketizer.push(final_for(5), opts).frame.is_some());
    }
}
