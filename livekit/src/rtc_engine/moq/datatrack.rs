// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use bytes::Bytes;
use moqtap_codec::{draft23::data_stream::DatagramHeader, varint::VarInt};

use super::Result;

const CHUNK: usize = 1024;
const MAX_CHUNKS: usize = 64;
const GROUP_WINDOW: u64 = 16;

/// Each serialized LiveKit packet has its own group. A lost object drops at
/// most that packet; subsequent groups need no acknowledgement or repair.
pub(super) fn send_packet(
    raw: &quinn::Connection,
    alias: VarInt,
    group: u64,
    packet: &[u8],
) -> Result<()> {
    if packet.is_empty() || packet.len() > MAX_CHUNKS * CHUNK {
        return Ok(());
    }
    let group_id = VarInt::from_u64(group)?;
    for (i, chunk) in packet.chunks(CHUNK).enumerate() {
        let last = (i + 1) * CHUNK >= packet.len();
        let header = DatagramHeader {
            datagram_type: if last { 0x02 } else { 0 }, // END_OF_GROUP
            track_alias: alias,
            group_id,
            object_id: VarInt::from_u64(i as u64)?,
            publisher_priority: Some(128),
            properties: Vec::new(),
            object_status: None,
        };
        let mut buf = Vec::with_capacity(32 + chunk.len());
        header.encode_checked(&mut buf)?;
        buf.extend_from_slice(chunk);
        match raw.send_datagram(Bytes::from(buf)) {
            Ok(()) => {}
            Err(quinn::SendDatagramError::TooLarge) => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[derive(Default)]
struct Group {
    parts: BTreeMap<usize, Vec<u8>>,
    last: Option<usize>,
    delivered: bool,
}

/// Per-alias receive window. Completed IDs remain as tombstones to discard duplicates.
#[derive(Default)]
pub(super) struct Assembler {
    groups: BTreeMap<u64, Group>,
    highest: u64,
}

impl Assembler {
    pub(super) fn push(&mut self, header: &DatagramHeader, payload: Vec<u8>) -> Option<Bytes> {
        let id = header.group_id.into_inner();
        let index = usize::try_from(header.object_id.into_inner()).ok()?;
        let end = header.is_end_of_group();
        if header.has_status()
            || index >= MAX_CHUNKS
            || payload.is_empty()
            || payload.len() > CHUNK
            || (!end && (payload.len() != CHUNK || index + 1 == MAX_CHUNKS))
            || self.highest.saturating_sub(id) >= GROUP_WINDOW
        {
            return None;
        }
        if id > self.highest {
            self.highest = id;
            while self.groups.first_key_value().is_some_and(|(&k, _)| id - k >= GROUP_WINDOW) {
                self.groups.pop_first();
            }
        }
        let group = self.groups.entry(id).or_default();
        if group.delivered
            || group.parts.contains_key(&index)
            || group.last.is_some_and(|last| index > last || end)
        {
            return None;
        }
        if end {
            if group.parts.last_key_value().is_some_and(|(&k, _)| k > index) {
                return None;
            }
            group.last = Some(index);
        }
        group.parts.insert(index, payload);
        if group.last? + 1 != group.parts.len() {
            return None;
        }
        group.delivered = true;
        let parts = std::mem::take(&mut group.parts);
        if parts.len() == 1 {
            return parts.into_values().next().map(Bytes::from);
        }
        Some(Bytes::from(parts.into_values().flatten().collect::<Vec<_>>()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(group: u32, index: u32, last: bool) -> DatagramHeader {
        DatagramHeader {
            datagram_type: if last { 2 } else { 0 },
            group_id: group.into(),
            object_id: index.into(),
            track_alias: 1u32.into(),
            publisher_priority: Some(128),
            properties: vec![],
            object_status: None,
        }
    }

    #[test]
    fn incomplete_groups_do_not_block_later_packets() {
        let mut a = Assembler::default();
        assert!(a.push(&object(0, 0, false), vec![0; CHUNK]).is_none());
        assert!(a.push(&object(1, 1, true), vec![2]).is_none());
        assert!(a.push(&object(1, 1, true), vec![2]).is_none());
        let packet = a.push(&object(1, 0, false), vec![1; CHUNK]).unwrap();
        assert_eq!(packet.len(), CHUNK + 1);
        assert_eq!(packet[CHUNK], 2);
        assert!(a.push(&object(1, 0, false), vec![1; CHUNK]).is_none());
        assert!(a.push(&object(16, 0, true), vec![3]).is_some());
        assert!(a.push(&object(0, 1, true), vec![4]).is_none());
    }
}
