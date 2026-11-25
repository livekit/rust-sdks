//! Packet-local adaptive binary rANS. Models reset for every frame so references never carry entropy state.
use anyhow::{ensure, Context, Result};

const TOTAL: u32 = 4096;
const LOWER: u32 = 1 << 23;

/// Separate binary trees learn the distributions of each kind of mesh or byte residual.
#[derive(Clone, Copy)]
pub(crate) enum Field {
    VertexMoved,
    VertexX,
    VertexY,
    ColorChanged,
    Red,
    Green,
    Blue,
    FaceIndex,
    FaceAgreement,
    DetailPresent,
    SplitAdded,
    SplitRetained,
    CornerIndex,
    TrackedCorner,
    ByteChanged,
    ByteHigh,
    ByteLow,
    FaceChanged,
    LayerChanged,
    WideCoordinate,
    LayerReference,
    LayerReferenceOffset,
    SameLayerFill,
    SameLayerShape,
    EvenSteps,
    PaletteLength,
}
const MODELS: usize = (Field::PaletteLength as usize + 1) * 64;

/// Each binary tree has up to six levels, sufficient for a green RGB565 component.
pub(crate) trait Symbols {
    /// Writes or reads a bit from the specified adaptive model.
    fn bit(&mut self, model: usize, value: bool) -> Result<bool>;

    /// Codes a small integer from its most significant bit, conditioning on the prefix.
    fn number(&mut self, field: Field, bits: u8, value: u8) -> Result<u8> {
        let mut node = 1usize;
        for shift in (0..bits).rev() {
            let bit = self.bit(field as usize * 64 + node, value & (1 << shift) != 0)?;
            node = node * 2 + usize::from(bit);
        }
        Ok((node - (1 << bits)) as u8)
    }

    /// Codes one binary decision.
    fn flag(&mut self, field: Field, value: bool) -> Result<bool> {
        self.bit(field as usize * 64 + 1, value)
    }
}

/// Codes a modular component difference in order of increasing distance from its predictor.
pub(crate) fn component(
    io: &mut impl Symbols,
    field: Field,
    bits: u8,
    value: u8,
    prediction: u8,
) -> Result<u8> {
    let mask = (1u8 << bits) - 1;
    let delta = value.wrapping_sub(prediction) & mask;
    let rank = if delta <= mask / 2 { delta * 2 } else { (mask - delta) * 2 + 1 };
    let rank = io.number(field, bits, rank)?;
    let delta = if rank & 1 == 0 { rank / 2 } else { mask - rank / 2 };
    Ok(prediction.wrapping_add(delta) & mask)
}

/// Predicts each RGB565 component independently.
pub(crate) fn rgb565(io: &mut impl Symbols, value: u16, prediction: u16) -> Result<u16> {
    let mut word = 0;
    for (field, shift, bits) in [(Field::Red, 11, 5), (Field::Green, 5, 6), (Field::Blue, 0, 5)] {
        let channel = |word: u16| ((word >> shift) & ((1 << bits) - 1)) as u8;
        word |=
            u16::from(component(io, field, bits, channel(value), channel(prediction))?) << shift;
    }
    Ok(word)
}

/// Counts binary outcomes with a bounded history.
#[derive(Clone, Copy)]
struct Model {
    zero: u16,
    total: u16,
}

impl Model {
    const NEW: Self = Self { zero: 1, total: 2 };

    /// Maps observed counts to an interval on the common 12-bit probability scale.
    fn split(self) -> u32 {
        u32::from(self.zero) * TOTAL / u32::from(self.total)
    }

    /// Updates the frequencies after coding one bit.
    fn observe(&mut self, one: bool) {
        self.zero += u16::from(!one);
        self.total += 1;
        if self.total >= 256 {
            let ones = (self.total - self.zero).div_ceil(2);
            self.zero = self.zero.div_ceil(2);
            self.total = self.zero + ones;
        }
    }
}

/// Seeds small temporal residuals without carrying any model state between packets.
fn models(compact: bool) -> [Model; MODELS] {
    let mut models = [Model::NEW; MODELS];
    if compact {
        for (field, bits) in [
            (Field::VertexX, 4),
            (Field::VertexY, 4),
            (Field::Red, 5),
            (Field::Green, 6),
            (Field::Blue, 5),
            (Field::FaceIndex, 4),
            (Field::FaceAgreement, 4),
            (Field::TrackedCorner, 4),
            (Field::CornerIndex, 4),
        ] {
            // A zero prefix is likely for a small residual. Lower bits are less predictable.
            for level in 0..bits {
                let zeros = 1 << (bits - level).min(3);
                models[field as usize * 64 + (1 << level)] =
                    Model { zero: zeros, total: zeros + 1 };
            }
        }
    }
    models
}

/// Buffers bounded symbol intervals in forward order for reverse rANS emission.
pub(crate) struct Writer {
    models: [Model; MODELS],
    intervals: Vec<(u16, u16)>,
}

impl Writer {
    /// Starts an independent entropy stream.
    pub(crate) fn new(compact: bool) -> Self {
        Self { models: models(compact), intervals: Vec::new() }
    }

    /// Emits the initial decoder state followed by byte renormalizations.
    pub(crate) fn finish(self, out: &mut Vec<u8>) {
        let start = out.len();
        let mut state = LOWER;
        for (base, freq) in self.intervals.into_iter().rev() {
            let freq = u32::from(freq);
            let limit = ((LOWER / TOTAL) << 8) * freq;
            while state >= limit {
                out.push(state as u8);
                state >>= 8;
            }
            state = (state / freq) * TOTAL + state % freq + u32::from(base);
        }
        out.extend_from_slice(&state.to_be_bytes());
        out[start..].reverse();
    }
}

impl Symbols for Writer {
    fn bit(&mut self, model: usize, value: bool) -> Result<bool> {
        let m = &mut self.models[model];
        let split = m.split();
        let (base, freq) = if value { (split, TOTAL - split) } else { (0, split) };
        self.intervals.push((base as u16, freq as u16));
        m.observe(value);
        Ok(value)
    }
}

/// Reads only the bytes belonging to this packet.
pub(crate) struct Reader<'a> {
    models: [Model; MODELS],
    state: u32,
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Checks the normalized initial rANS state before reading any symbols.
    pub(crate) fn new(data: &'a [u8], compact: bool) -> Result<Self> {
        let (head, bytes) = data.split_first_chunk::<4>().context("Missing entropy state")?;
        let state = u32::from_le_bytes(*head);
        ensure!((LOWER..LOWER * 256).contains(&state), "Invalid entropy state");
        Ok(Self { models: models(compact), state, bytes })
    }

    /// Rejects incomplete, surplus and incorrectly terminated streams.
    pub(crate) fn finish(self) -> Result<()> {
        ensure!(self.state == LOWER && self.bytes.is_empty(), "Invalid entropy stream termination");
        Ok(())
    }
}

impl Symbols for Reader<'_> {
    fn bit(&mut self, model: usize, _: bool) -> Result<bool> {
        let m = &mut self.models[model];
        let split = m.split();
        let slot = self.state % TOTAL;
        let bit = slot >= split;
        let (base, freq) = if bit { (split, TOTAL - split) } else { (0, split) };
        self.state = freq * (self.state / TOTAL) + slot - base;
        while self.state < LOWER {
            let (&next, rest) = self.bytes.split_first().context("Truncated entropy stream")?;
            self.state = (self.state << 8) | u32::from(next);
            self.bytes = rest;
        }
        m.observe(bit);
        Ok(bit)
    }
}
