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

use crate::dtp::{Dtp, Extensions, FrameMarker};
use bytes::{Bytes, BytesMut};
use std::collections::BTreeMap;

/// Assembles packets into frames.
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
    pub fn push(&mut self, dtp: Dtp) -> Option<DepacketizerFrame> {
        match dtp.header.marker {
            FrameMarker::Single => self.frame_from_single(dtp).into(),
            FrameMarker::Start => {
                self.begin_partial(dtp);
                None
            }
            FrameMarker::Inter => {
                self.push_to_partial(dtp);
                None
            }
            FrameMarker::Final => {
                self.push_to_partial(dtp);
                self.finalize_partial()
            }
        }
    }

    fn frame_from_single(&mut self, dtp: Dtp) -> DepacketizerFrame {
        debug_assert!(dtp.header.marker == FrameMarker::Single);

        if self.partial.is_some() {
            println!("Drop: interrupted");
            self.partial = None;
        }
        DepacketizerFrame { payload: dtp.payload, extensions: dtp.header.extensions }
    }

    /// Begin assembling a new packet.
    fn begin_partial(&mut self, dtp: Dtp) {
        debug_assert!(dtp.header.marker == FrameMarker::Start);

        if self.partial.is_some() {
            println!("Drop: interrupted");
            self.partial = None;
        }
        let start_sequence = dtp.header.sequence;
        let payload_len = dtp.payload.len();

        let partial = PartialFrame {
            frame_number: dtp.header.frame_number,
            start_sequence,
            end_sequence: None,
            extensions: dtp.header.extensions,
            payloads: BTreeMap::from([(start_sequence, dtp.payload)]),
            payload_len,
        };
        self.partial = partial.into();
    }

    /// Push to the existing partial frame.
    fn push_to_partial(&mut self, dtp: Dtp) {
        debug_assert!(matches!(dtp.header.marker, FrameMarker::Inter | FrameMarker::Final));

        let Some(mut partial) = self.partial.take() else {
            println!("Drop: unknown frame");
            return;
        };
        if dtp.header.frame_number != partial.frame_number {
            println!("Drop: interrupted");
            return;
        }
        if partial.payloads.len() == Self::MAX_BUFFER_PACKETS {
            println!("Drop: buffer full");
            return;
        }

        partial.payload_len += dtp.payload.len();
        partial.payloads.insert(dtp.header.sequence, dtp.payload);

        if dtp.header.marker == FrameMarker::Final {
            partial.end_sequence = dtp.header.sequence.into();
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

    #[test]
    fn test_single_packet() {
        let mut depacketizer = Depacketizer::new();

        let mut dtp: Dtp = Faker.fake();
        dtp.header.marker = FrameMarker::Single;

        let frame = depacketizer.push(dtp.clone()).unwrap();
        assert_eq!(frame.payload, dtp.payload);
        assert_eq!(frame.extensions, dtp.header.extensions);
    }

    #[test]
    fn test_multi_packet() {
        const INTER_FRAMES: usize = 8;
        let mut depacketizer = Depacketizer::new();

        let mut dtp: Dtp = Faker.fake();
        dtp.header.marker = FrameMarker::Start;

        assert!(depacketizer.push(dtp.clone()).is_none());

        for _ in 0..INTER_FRAMES {
            dtp.header.marker = FrameMarker::Inter;
            dtp.header.sequence += 1;
            assert!(depacketizer.push(dtp.clone()).is_none());
        }

        dtp.header.marker = FrameMarker::Final;
        dtp.header.sequence += 1;

        let frame = depacketizer.push(dtp.clone()).unwrap();
        assert_eq!(frame.extensions, dtp.header.extensions);
        assert_eq!(frame.payload.len(), dtp.payload.len() * (INTER_FRAMES + 2)); // 1 start, 8 inter, 1 end
    }

    impl fake::Dummy<fake::Faker> for Dtp {
        fn dummy_with_rng<R: rand::Rng + ?Sized>(_: &fake::Faker, rng: &mut R) -> Self {
            let payload_len = rng.random_range(0..=1500);
            let payload = (0..payload_len).map(|_| rng.random()).collect::<Bytes>();
            Self { header: Faker.fake_with_rng(rng), payload }
        }
    }
}
