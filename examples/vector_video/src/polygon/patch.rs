//! Sparse changes to the four-bit colors of a reference mesh with identical geometry.
use super::*;

/// Writes bits most significant first, with zero padding at the end.
struct Bits {
    bytes: Vec<u8>,
    used: usize,
}

impl Bits {
    /// Appends the low `width` bits of a number.
    fn put(&mut self, value: usize, width: usize) {
        for shift in (0..width).rev() {
            if self.used.is_multiple_of(8) {
                self.bytes.push(0);
            }
            self.bytes[self.used / 8] |= (((value >> shift) & 1) << (7 - self.used % 8)) as u8;
            self.used += 1;
        }
    }
}

/// Reads bounded integers from the payload without buffering decoded changes.
struct Cursor<'a> {
    bytes: &'a [u8],
    used: usize,
}

impl Cursor<'_> {
    /// Reads a fixed-width unsigned integer.
    fn get(&mut self, width: usize) -> Result<usize> {
        ensure!(self.used + width <= self.bytes.len() * 8, "Truncated mesh paint patch");
        let mut value = 0;
        for _ in 0..width {
            value = value * 2 + usize::from((self.bytes[self.used / 8] >> (7 - self.used % 8)) & 1);
            self.used += 1;
        }
        Ok(value)
    }
}

/// The average spacing of changed triangles sets the Rice remainder width.
fn rice_width(triangles: usize, changes: usize) -> usize {
    (triangles / changes).max(1).ilog2() as usize
}

/// Reads one palette slot from the canonical CV body.
fn nibble(body: &[u8], slot: usize) -> u8 {
    (body[slot / 2] >> (4 * (1 - slot % 2))) & 15
}

impl Mesh {
    /// Enumerates the raw color nibbles in face order, with each split center followed by its three corners.
    fn paint_slots(&self) -> impl Iterator<Item = usize> + '_ {
        let faces = self.grid.face_count();
        let base = (self.grid.descriptor_bytes() + self.grid.vertices() + 32) * 2;
        let mut corner = base + faces + faces.div_ceil(8) * 2;
        (0..faces).flat_map(move |i| {
            let split = self.detail.get(i).is_some_and(Option::is_some);
            let slots = [base + i, corner, corner + 1, corner + 2];
            corner += if split { 3 } else { 0 };
            slots.into_iter().take(if split { 4 } else { 1 })
        })
    }

    /// Returns the number of addressable leading slots, leaving trailing copies implicit.
    pub(super) fn palette_prefix_len(&self) -> Option<usize> {
        let FaceColors::Indexed { palette, .. } = &self.colors else { return None };
        Some(palette.iter().rposition(|&word| word != palette[0]).map_or(1, |i| i + 1))
    }

    /// Tries a list of color replacements when all other canonical bytes can be inherited.
    pub(crate) fn paint_patch(&self, raw: &[u8], base: &Self, reference: &[u8]) -> Option<Vec<u8>> {
        let size = self.palette_prefix_len()?;
        if size < 2 || raw.len() != reference.len() || self.grid != base.grid {
            return None;
        }
        let colors = self.grid.descriptor_bytes() + self.grid.vertices() + 32;
        let mask = colors + self.grid.face_count() / 2;
        let end =
            mask + if self.detail.is_empty() { 0 } else { self.grid.face_count().div_ceil(8) };
        if raw[..colors] != reference[..colors]
            || raw[mask..end] != reference[mask..end]
            || self.detail.is_empty() != base.detail.is_empty()
        {
            return None;
        }
        let edits = || {
            self.paint_slots().enumerate().filter_map(|(i, slot)| {
                let from = usize::from(nibble(reference, slot));
                let to = usize::from(nibble(raw, slot));
                (from != to).then_some((i, from, to))
            })
        };
        let mut count = edits().count();
        if count == 0 {
            return None;
        }
        let width = rice_width(self.len(), count);
        let mut out = Vec::new();
        while count >= 128 {
            out.push((count as u8 & 127) | 128);
            count >>= 7;
        }
        out.push(count as u8);
        // Truncated binary omits the previous slot from the replacement alphabet.
        let color_bits = (size - 1).ilog2() as usize;
        let short = (2 << color_bits) - (size - 1);
        let mut bits = Bits { used: out.len() * 8, bytes: out };
        let mut next = 0;
        for (i, from, to) in edits() {
            if from >= size || to >= size {
                return None;
            }
            let rank = to - usize::from(to > from);
            let gap = i - next;
            next = i + 1;
            for _ in 0..gap >> width {
                bits.put(0, 1);
            }
            bits.put(1, 1);
            bits.put(gap, width);
            if rank < short {
                bits.put(rank, color_bits);
            } else {
                bits.put(rank + short, color_bits + 1);
            }
        }
        Some(bits.bytes)
    }

    /// Applies a validated patch to a private copy of the reference body.
    pub(crate) fn apply_paint(&self, data: &[u8], body: &mut [u8]) -> Result<()> {
        let size = self.palette_prefix_len().context("Paint patch requires indexed colors")?;
        ensure!(size >= 2, "Paint patch requires two palette slots");
        let triangles = self.len();
        let mut data = data.iter();
        let mut count = 0;
        for shift in (0..usize::BITS).step_by(7) {
            let &b = data.next().context("Missing mesh paint count")?;
            let digit = usize::from(b & 127);
            ensure!(digit <= triangles >> shift, "Invalid mesh paint count");
            count |= digit << shift;
            if b < 128 {
                ensure!(shift == 0 || digit != 0, "Noncanonical mesh paint count");
                break;
            }
            ensure!(shift + 7 < usize::BITS, "Invalid mesh paint count");
        }
        ensure!((1..=triangles).contains(&count), "Invalid mesh paint count");
        let width = rice_width(triangles, count);
        let color_bits = (size - 1).ilog2() as usize;
        let short = (2 << color_bits) - (size - 1);
        let mut bits = Cursor { bytes: data.as_slice(), used: 0 };
        let mut slots = self.paint_slots();
        for _ in 0..count {
            let mut quotient = 0;
            while bits.get(1)? == 0 {
                quotient += 1;
                ensure!(quotient <= triangles >> width, "Invalid mesh paint gap");
            }
            let gap = (quotient << width) + bits.get(width)?;
            let slot = slots.nth(gap).context("Mesh paint outside reference")?;
            let from = usize::from(nibble(body, slot));
            ensure!(from < size, "Invalid mesh paint reference slot");
            let prefix = bits.get(color_bits)?;
            let rank = if prefix < short { prefix } else { prefix * 2 + bits.get(1)? - short };
            let to = (rank + usize::from(rank >= from)) as u8;
            let shift = 4 * (1 - slot % 2);
            body[slot / 2] = (body[slot / 2] & !(15 << shift)) | to << shift;
        }
        let padding = (8 - bits.used % 8) % 8;
        ensure!(
            bits.get(padding)? == 0 && bits.used / 8 == bits.bytes.len(),
            "Invalid mesh paint padding"
        );
        Ok(())
    }
}
