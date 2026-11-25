//! Delivery of complete codec snapshots across bounded Data Track reassembly buffers.
use anyhow::{ensure, Context, Result};

use crate::{
    codec::{DecodedFrame, MAX_FRAME_BYTES},
    compression::PacketDecoder,
};

// Leave room for the application and Data Track headers within the native 16,000-byte MTU.
const PART_BYTES: usize = 15_000;
const HEADER_BYTES: usize = 2 + 4 + 4 + 4;

/// Yields a single original snapshot or its indexed fragments in transmission order.
pub(crate) fn split(index: u32, mut frame: Vec<u8>) -> impl ExactSizeIterator<Item = Vec<u8>> {
    let count = frame.len().div_ceil(PART_BYTES).max(1);
    (0..count).map(move |part| {
        if count == 1 {
            return std::mem::take(&mut frame);
        }
        let start = part * PART_BYTES;
        let payload = &frame[start..(start + PART_BYTES).min(frame.len())];
        let mut message = Vec::with_capacity(HEADER_BYTES + payload.len());
        message.extend_from_slice(b"CF");
        message.extend_from_slice(&index.to_le_bytes());
        message.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        message.extend_from_slice(&(part as u32).to_le_bytes());
        message.extend_from_slice(payload);
        message
    })
}

/// Fragments of the newest incomplete picture, stored only as their payloads arrive.
struct PartialFrame {
    index: u32,
    bytes: usize,
    parts: Vec<Option<Vec<u8>>>,
    received: usize,
}

/// Reassembles a complete picture before committing it to the predictive decoder.
#[derive(Default)]
pub(crate) struct Receiver {
    decoder: PacketDecoder,
    partial: Option<PartialFrame>,
}

impl Receiver {
    /// Returns the most recent decoded snapshot for the lossless benchmark.
    pub(crate) fn last_snapshot(&self) -> Option<&[u8]> {
        self.decoder.last_snapshot()
    }

    /// Drops missing or superseded pictures and accepts out-of-order parts of the current frame.
    pub(crate) fn decode(&mut self, message: &[u8]) -> Result<Option<DecodedFrame>> {
        if !message.starts_with(b"CF") {
            let batch = self.decoder.decode(message)?;
            if let (Some(batch), Some(partial)) = (&batch, &self.partial) {
                if batch.start.wrapping_sub(partial.index) < 1 << 31 {
                    self.partial = None;
                }
            }
            return Ok(batch);
        }
        let (header, payload) =
            message.split_at_checked(HEADER_BYTES).context("Truncated fragment")?;
        let index = u32::from_le_bytes(header[2..6].try_into()?);
        let bytes = u32::from_le_bytes(header[6..10].try_into()?) as usize;
        let number = u32::from_le_bytes(header[10..14].try_into()?) as usize;
        ensure!(
            (PART_BYTES + 1..=MAX_FRAME_BYTES).contains(&bytes),
            "Invalid fragmented frame size"
        );
        let count = bytes.div_ceil(PART_BYTES);
        ensure!(
            number < count && payload.len() == PART_BYTES.min(bytes - number * PART_BYTES),
            "Invalid frame fragment"
        );
        if self.partial.as_ref().is_some_and(|p| index.wrapping_sub(p.index) >= 1 << 31) {
            return Ok(None);
        }
        if self.partial.as_ref().is_none_or(|p| p.index != index) {
            self.partial =
                Some(PartialFrame { index, bytes, parts: vec![None; count], received: 0 });
        }
        let pending = self.partial.as_mut().expect("partial frame initialized");
        ensure!(pending.bytes == bytes, "Inconsistent fragment lengths");
        if pending.parts[number].is_some() {
            return Ok(None);
        }
        pending.parts[number] = Some(payload.to_vec());
        pending.received += 1;
        if pending.received != count {
            return Ok(None);
        }
        let complete = self.partial.take().expect("all parts received");
        let mut frame = Vec::with_capacity(complete.bytes);
        for part in complete.parts.into_iter().flatten() {
            frame.extend_from_slice(&part);
        }
        let index_offset = match frame.get(..2) {
            Some(b"CV" | b"CI" | b"CK") => 3,
            Some(b"CD" | b"CR" | b"CT" | b"CM" | b"CX") => 2,
            _ if crate::compression::short_mesh_magic(&frame).is_some() => 1,
            _ => anyhow::bail!("Invalid fragmented picture"),
        };
        ensure!(
            frame.get(index_offset..index_offset + 4) == Some(index.to_le_bytes().as_slice()),
            "Inconsistent fragment frame index"
        );
        self.decoder.decode(&frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Constructs a legal 2048×1152 raw mesh larger than one MiB.
    fn large_picture(index: u32) -> Vec<Vec<u8>> {
        let (cols, rows) = (1024u16, 520u16);
        let mut frame = b"CV\x86".to_vec();
        frame.extend_from_slice(&index.to_le_bytes());
        frame.extend_from_slice(&[30, 0, 1, 0, 1]);
        for v in [2048u16, 1152] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        frame.push(u8::MAX);
        for v in [cols, rows] {
            frame.extend_from_slice(&v.to_le_bytes());
        }
        let (w, h) = (usize::from(cols), usize::from(rows));
        frame.resize(frame.len() + (w + 1) * (h + 1), 0x77);
        frame.resize(frame.len() + 32 + w * h, 0);
        split(index, frame).collect()
    }

    /// Lost, out-of-order, duplicate and corrupt fragments never advance a partial prediction.
    #[test]
    fn reassembles_large_mesh_and_recovers_after_loss() {
        let a = large_picture(1);
        let b = large_picture(2);
        let mut receiver = Receiver::default();
        assert!(receiver.decode(&a[0]).unwrap().is_none());
        assert!(receiver.decode(&b[1]).unwrap().is_none());
        assert!(receiver.decode(&b[1]).unwrap().is_none());
        assert!(receiver.decode(&a[1]).unwrap().is_none());
        assert!(receiver.decode(&b[0][..40]).is_err());
        assert!(receiver.last_snapshot().is_none());
        for part in b[2..].iter().rev() {
            assert!(receiver.decode(part).unwrap().is_none());
        }
        let batch = receiver.decode(&b[0]).unwrap().unwrap();
        assert_eq!(batch.start, 2);
        assert_eq!(batch.polygon_count(), 2 * 1024 * 520);
        let mut corrupt = b[1].clone();
        corrupt[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(receiver.decode(&corrupt).is_err());
    }
}
