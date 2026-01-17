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

use crate::packet::{Packet, Extensions, FrameMarker};
use bytes::{Bytes, BytesMut};
use std::collections::BTreeMap;

/// Reassembles packets into frames.
#[derive(Debug)]
pub struct Depacketizer {
    /// Partial frame currently being assembled.
    partial: Option<PartialFrame>,
}

/// Output of [`Depacketizer`].
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

    /// Push a packet into the depacketizer, returning a complete frame if one is available.
    pub fn push(&mut self, packet: Packet) -> Option<DepacketizerFrame> {
        match packet.header.marker {
            FrameMarker::Single => self.frame_from_single(packet).into(),
            FrameMarker::Start => {
                self.begin_partial(packet);
                None
            }
            FrameMarker::Inter => {
                self.push_to_partial(packet);
                None
            }
            FrameMarker::Final => {
                self.push_to_partial(packet);
                self.finalize_partial()
            }
        }
    }

    fn frame_from_single(&mut self, packet: Packet) -> DepacketizerFrame {
        debug_assert!(packet.header.marker == FrameMarker::Single);

        if self.partial.is_some() {
            log::trace!("Drop: interrupted");
            self.partial = None;
        }
        DepacketizerFrame { payload: packet.payload, extensions: packet.header.extensions }
    }

    /// Begin assembling a new packet.
    fn begin_partial(&mut self, packet: Packet) {
        debug_assert!(packet.header.marker == FrameMarker::Start);

        if self.partial.is_some() {
            log::trace!("Drop: interrupted");
            self.partial = None;
        }
        let start_sequence = packet.header.sequence;
        let payload_len = packet.payload.len();

        let partial = PartialFrame {
            frame_number: packet.header.frame_number,
            start_sequence,
            end_sequence: None,
            extensions: packet.header.extensions,
            payloads: BTreeMap::from([(start_sequence, packet.payload)]),
            payload_len,
        };
        self.partial = partial.into();
    }

    /// Push to the existing partial frame.
    fn push_to_partial(&mut self, packet: Packet) {
        debug_assert!(matches!(packet.header.marker, FrameMarker::Inter | FrameMarker::Final));

        let Some(mut partial) = self.partial.take() else {
            log::trace!("Drop: unknown frame");
            return;
        };
        if packet.header.frame_number != partial.frame_number {
            log::trace!("Drop: interrupted");
            return;
        }
        if partial.payloads.len() == Self::MAX_BUFFER_PACKETS {
            log::trace!("Drop: buffer full");
            return;
        }

        partial.payload_len += packet.payload.len();
        partial.payloads.insert(packet.header.sequence, packet.payload);

        if packet.header.marker == FrameMarker::Final {
            partial.end_sequence = packet.header.sequence.into();
        }

        self.partial = Some(partial);
    }

    /// If there is a partial frame and it has an end sequence set,
    /// return a complete frame from it.
    ///
    fn finalize_partial(&mut self) -> Option<DepacketizerFrame> {
        let Some(mut partial) = self.partial.take() else {
            log::trace!("Drop: unknown frame");
            return None;
        };
        let Some(end_sequence) = partial.end_sequence else {
            log::trace!("Drop: no end sequence");
            return None;
        };

        let mut sequence = partial.start_sequence;
        let mut payload = BytesMut::with_capacity(partial.payload_len);

        while let Some(partial_payload) = partial.payloads.remove(&sequence) {
            debug_assert!(payload.len() + partial_payload.len() <= payload.capacity());
            payload.extend(partial_payload);

            if sequence < end_sequence {
                sequence = sequence.wrapping_add(1);
                continue;
            }
            return DepacketizerFrame { payload: payload.freeze(), extensions: partial.extensions }
                .into();
        }
        None
    }
}

#[derive(Debug)]
struct PartialFrame {
    /// Frame number from the start packet.
    frame_number: u16,
    /// Sequence of the start packet.
    start_sequence: u16,
    /// End sequence, if final packet has been received.
    end_sequence: Option<u16>,
    /// Extensions from the start packet.
    extensions: Extensions,
    /// Mapping between sequence number and packet payload.
    payloads: BTreeMap<u16, Bytes>,
    /// Sum of payload lengths.
    payload_len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake::{Fake, Faker};
    use test_case::test_case;

    #[test]
    fn test_single_packet() {
        let mut depacketizer = Depacketizer::new();

        let mut packet: Packet = Faker.fake();
        packet.header.marker = FrameMarker::Single;

        let frame = depacketizer.push(packet.clone()).unwrap();
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

        assert!(depacketizer.push(packet.clone()).is_none());

        for _ in 0..inter_packets {
            packet.header.marker = FrameMarker::Inter;
            packet.header.sequence += 1;
            assert!(depacketizer.push(packet.clone()).is_none());
        }

        packet.header.marker = FrameMarker::Final;
        packet.header.sequence += 1;

        let frame = depacketizer.push(packet.clone()).unwrap();
        assert_eq!(frame.extensions, packet.header.extensions);
        assert_eq!(frame.payload.len(), packet.payload.len() * (inter_packets + 2));
    }

    impl fake::Dummy<fake::Faker> for Packet {
        fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
            let payload_len = rng.random_range(0..=1500);
            let payload = (0..payload_len).map(|_| rng.random()).collect::<Bytes>();
            Self { header: Faker.fake_with_rng(rng), payload }
        }
    }
}
