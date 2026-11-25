//! Lossless per-message compression with bounded temporal dependencies.
pub(crate) mod entropy;
mod layered;

use std::collections::HashMap;

use anyhow::{ensure, Context, Result};
use clap::ValueEnum;

use crate::{
    codec::{
        check_size, has_stroke_records, header_len, packet_canvas, DecodedFrame, WireFormat,
        MAX_FRAME_BYTES,
    },
    layers::{is_stroke_record, RECORD},
    polygon::{Grid, Mesh, MeshCoding},
};
use entropy::{Field, Reader, Symbols, Writer};

/// Normal distance between packets decodable from just the independent anchor.
const RECOVERY_STRIDE: u32 = 4;

/// Selects the lossless representation of a finished polygon frame.
#[derive(Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum Compression {
    /// Send the original self-contained CV packets, compatible with earlier subscribers.
    Off,
    /// Entropy-code each packet independently.
    Spatial,
    /// Predict from recent packets and send periodic independent refreshes.
    #[default]
    Predictive,
    /// Also try temporal deltas that skip unchanged polygons and favor small residuals.
    Compact,
    /// Also reuse nearby layer records and shapes that only changed position.
    CompactMotion,
    /// Also try sparse color replacements and a predictor for coarse mesh motion and shared palettes.
    MeshPatch,
}

/// Selects the body predictor identified by the wire packet.
enum DeltaCodec {
    Standard,
    Compact,
    Motion,
    MeshPatch,
}

/// A validated reference in the original CV representation, plus parsed mesh predictors.
struct Snapshot {
    raw: Vec<u8>,
    header: usize,
    index: u32,
    mesh: Option<Mesh>,
}

impl Snapshot {
    /// Reads known version and geometry before installing a reference.
    fn new(raw: Vec<u8>) -> Result<Self> {
        ensure!(raw.starts_with(b"CV"), "Invalid snapshot");
        let &version = raw.get(2).context("Missing version")?;
        let format = WireFormat::try_from(version & 0x7f)?;
        let canvas = packet_canvas(&raw)?;
        check_size(version, raw.len(), canvas)?;
        let header = header_len(version);
        let body = raw.get(header..).context("Truncated snapshot header")?;
        let mesh = if format == WireFormat::ColoredPolygon {
            Some(Mesh::decode(body, canvas)?)
        } else {
            None
        };
        let index = u32::from_le_bytes(raw[3..7].try_into()?);
        Ok(Self { raw, header, index, mesh })
    }

    /// Checks that a dependent packet can inherit all metadata other than the source index.
    fn compatible(&self, other: &Self) -> bool {
        self.raw[2] == other.raw[2]
            && self.raw[7..self.header] == other.raw[7..other.header]
            && self.mesh.as_ref().map(|m| m.grid) == other.mesh.as_ref().map(|m| m.grid)
    }

    /// Returns the payload following the snapshot header.
    fn body(&self) -> &[u8] {
        &self.raw[self.header..]
    }

    /// Encodes spatial or temporal fields using the specified mesh predictor.
    fn candidate(&self, previous: Option<&Self>, coding: MeshCoding) -> Result<Vec<u8>> {
        let compact = coding != MeshCoding::Standard;
        let mut out = Vec::new();
        if let Some(base) = previous {
            out.extend_from_slice(match coding {
                MeshCoding::Standard => b"CD",
                MeshCoding::Compact => b"CT",
                MeshCoding::Patch => b"CX",
            });
            out.extend_from_slice(&self.index.to_le_bytes());
            out.push(u8::try_from(self.index.wrapping_sub(base.index))?);
            if self.body() == base.body() {
                out[1] = b'R';
                return Ok(out);
            }
        } else {
            out.extend_from_slice(&self.raw[..self.header]);
            out[1] = if coding == MeshCoding::Patch { b'K' } else { b'I' };
        }
        let mut writer = Writer::new(compact);
        if let Some(mesh) = &self.mesh {
            if previous.is_none() {
                mesh.grid.write(&mut out);
            }
            Mesh::predict(
                &mut writer,
                mesh.grid,
                Some(mesh),
                previous.and_then(|s| s.mesh.as_ref()),
                coding,
            )?;
        } else {
            out.extend_from_slice(&(self.body().len() as u16).to_le_bytes());
            let reference = previous.map(Self::body);
            if compact {
                layered::predict::<false>(
                    &mut writer,
                    self.body().len(),
                    self.body(),
                    reference.context("Missing compact layer reference")?,
                )?;
            } else {
                bytes(&mut writer, self.body().len(), self.body(), reference)?;
            }
        }
        writer.finish(&mut out);
        Ok(out)
    }

    /// Packs a mesh repeat or a sparse recoloring with its absolute frame index.
    fn paint_candidate(&self, base: &Self) -> Option<Vec<u8>> {
        let mesh = self.mesh.as_ref()?;
        let age = u8::try_from(self.index.wrapping_sub(base.index)).ok()?;
        let repeat = self.body() == base.body();
        let body = if repeat {
            Vec::new()
        } else {
            mesh.paint_patch(self.body(), base.mesh.as_ref()?, base.body())?
        };
        let mut packet = Vec::with_capacity(5 + body.len());
        packet.push(if repeat { 0xc0 | age } else { 0x80 | age });
        packet.extend_from_slice(&self.index.to_le_bytes());
        packet.extend(body);
        Some(packet)
    }

    /// Encodes a layered delta that can refer to records in different drawing positions.
    fn motion_candidate(&self, base: &Self) -> Result<Vec<u8>> {
        let mut out = b"CM".to_vec();
        out.extend_from_slice(&self.index.to_le_bytes());
        out.push(u8::try_from(self.index.wrapping_sub(base.index))?);
        out.extend_from_slice(&(self.body().len() as u16).to_le_bytes());
        let mut writer = Writer::new(true);
        layered::predict::<true>(&mut writer, self.body().len(), self.body(), base.body())?;
        writer.finish(&mut out);
        Ok(out)
    }
}

/// Predicts other packet bodies by byte position, with separate models for the residual nibbles.
fn bytes(
    io: &mut impl Symbols,
    len: usize,
    target: &[u8],
    reference: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut decoded = Vec::with_capacity(len);
    for i in 0..len {
        let pred = reference
            .and_then(|r| r.get(i))
            .copied()
            .unwrap_or_else(|| decoded.last().copied().unwrap_or(0));
        let delta = target.get(i).copied().unwrap_or(0) ^ pred;
        let value = if io.flag(Field::ByteChanged, delta != 0)? {
            (io.number(Field::ByteHigh, 4, delta >> 4)? << 4
                | io.number(Field::ByteLow, 4, delta & 15)?)
                ^ pred
        } else {
            pred
        };
        decoded.push(value);
    }
    Ok(decoded)
}

/// Keeps just the last independent anchor and the most recently decoded dependent picture.
#[derive(Default)]
struct References {
    key: Option<Snapshot>,
    delta: Option<Snapshot>,
}

impl References {
    /// Returns the newest available picture.
    fn last(&self) -> Option<&Snapshot> {
        self.delta.as_ref().or(self.key.as_ref())
    }

    /// Selects a recoverable prediction base, or requests an independent refresh.
    fn prediction_for(&self, frame: &Snapshot, stretch_recovery: bool) -> Option<&Snapshot> {
        self.recoverable_base(frame.index, &frame.raw[7..11], stretch_recovery)
            .filter(|base| frame.compatible(base))
    }

    /// Finds the same reference for geometry preparation and for the packet predictor.
    fn recoverable_base(
        &self,
        index: u32,
        cadence: &[u8],
        stretch_recovery: bool,
    ) -> Option<&Snapshot> {
        let key = self.key.as_ref()?;
        let last = self.last()?;
        let &[a, b, c, d] = cadence else { return None };
        let age = index.wrapping_sub(key.index);
        let numerator = u16::from_le_bytes([a, b]);
        let denominator = u16::from_le_bytes([c, d]);
        if denominator == 0 || age == 0 || age >= u32::from(numerator.div_ceil(denominator)) {
            return None;
        }
        // A low-bandwidth mesh can defer recovery for up to two fifths of a second.
        let stride = if stretch_recovery {
            RECOVERY_STRIDE.max(2 * u32::from(numerator) / (5 * u32::from(denominator)))
        } else {
            RECOVERY_STRIDE
        };
        // Cross each recovery boundary using the independent anchor alone.
        let base =
            if age / stride != last.index.wrapping_sub(key.index) / stride { key } else { last };
        (index != base.index).then_some(base)
    }

    /// Commits state only after a picture has been validated.
    fn insert(&mut self, frame: Snapshot, independent: bool) {
        if independent {
            self.key = Some(frame);
            self.delta = None;
        } else {
            self.delta = Some(frame);
        }
    }
}

/// Chooses a lossless representation of the complete snapshot.
pub(crate) struct PacketEncoder {
    mode: Compression,
    refs: References,
    stretch_mesh_recovery: bool,
}

impl PacketEncoder {
    /// Starts a new stream with an empty reference set.
    pub(crate) fn new(mode: Compression) -> Self {
        Self { mode, refs: References::default(), stretch_mesh_recovery: false }
    }

    /// Trades a longer mesh packet-loss recovery window for smaller temporal residuals.
    pub(crate) fn with_longer_mesh_recovery(mut self) -> Self {
        self.stretch_mesh_recovery = true;
        self
    }

    /// Makes the next accepted send independently decodable after a local queue rejection.
    pub(crate) fn reset(&mut self) {
        self.refs = References::default();
    }

    /// Aligns closed layered outlines with the prediction reference.
    pub(crate) fn stabilize_outline_order(&self, raw: &mut [u8]) {
        if !matches!(
            self.mode,
            Compression::Predictive
                | Compression::Compact
                | Compression::CompactMotion
                | Compression::MeshPatch
        ) || !raw.starts_with(b"CV")
        {
            return;
        }
        let Some(&version) = raw.get(2) else { return };
        if version & 0x7f != WireFormat::LayeredPolygon as u8 || raw.len() < header_len(version) + 2
        {
            return;
        }
        let header = header_len(version);
        let index = u32::from_le_bytes([raw[3], raw[4], raw[5], raw[6]]);
        let Some(base) = self.refs.recoverable_base(index, &raw[7..11], false) else { return };
        if base.raw[2] != version || base.raw[7..base.header] != raw[7..header] {
            return;
        }
        let Some(reference_records) = base.body().get(2..) else { return };
        let references = reference_records.as_chunks::<RECORD>().0;
        let strokes = has_stroke_records(raw);
        let translated = (self.mode == Compression::CompactMotion).then(|| {
            let mut shapes = HashMap::<_, Vec<usize>>::new();
            for (index, record) in references.iter().enumerate() {
                if strokes && is_stroke_record(record) {
                    continue;
                }
                shapes.entry(&record[4..]).or_default().push(index);
            }
            shapes
        });
        for (index, record) in raw[header + 2..].as_chunks_mut::<RECORD>().0.iter_mut().enumerate()
        {
            if strokes && is_stroke_record(record) {
                continue;
            }
            if let Some(reference) = references.get(index) {
                align_vertices(&mut record[2..], &reference[2..]);
            }
            if let Some(shapes) = &translated {
                align_translated_outline(index, record, references, shapes);
            }
        }
    }

    /// Compresses the exact bytes already chosen by the visual encoder.
    pub(crate) fn pack(&mut self, raw: Vec<u8>) -> Result<Vec<u8>> {
        if self.mode == Compression::Off {
            return Ok(raw);
        }
        let frame = Snapshot::new(raw)?;
        let prediction = match self.mode {
            Compression::Predictive
            | Compression::Compact
            | Compression::CompactMotion
            | Compression::MeshPatch => {
                self.refs.prediction_for(&frame, self.stretch_mesh_recovery && frame.mesh.is_some())
            }
            _ => None,
        };
        let delta = if let Some(base) = prediction {
            let paint = if self.mode == Compression::MeshPatch {
                frame.paint_candidate(base)
            } else {
                None
            };
            // A small edit list can bypass full-frame entropy coding on both the delta and key paths.
            let paint = match paint {
                Some(packet) if packet.len() <= frame.raw.len() / 4 => {
                    self.refs.insert(frame, false);
                    return Ok(packet);
                }
                candidate => candidate,
            };
            let mut best = frame.candidate(Some(base), MeshCoding::Standard)?;
            if matches!(
                self.mode,
                Compression::Compact | Compression::CompactMotion | Compression::MeshPatch
            ) && !best.starts_with(b"CR")
            {
                let compact = frame.candidate(Some(base), MeshCoding::Compact)?;
                if compact.len() < best.len() {
                    best = compact;
                }
                if self.mode == Compression::CompactMotion && frame.mesh.is_none() {
                    let motion = frame.motion_candidate(base)?;
                    if motion.len() < best.len() {
                        best = motion;
                    }
                }
            }
            if self.mode == Compression::MeshPatch && frame.mesh.is_some() {
                let candidate = match paint {
                    Some(packet) => packet,
                    None => frame.candidate(Some(base), MeshCoding::Patch)?,
                };
                if candidate.len() < best.len() {
                    best = candidate;
                }
            }
            if self.mode == Compression::MeshPatch
                && frame.mesh.is_some()
                && frame.index == base.index.wrapping_add(1)
                && matches!(best.get(..2), Some(b"CD" | b"CT" | b"CX"))
            {
                // One-letter dense deltas imply the immediately preceding picture.
                best[0] = best[1];
                best.copy_within(2..6, 1);
                best.copy_within(7.., 5);
                best.truncate(best.len() - 2);
            }
            Some(best)
        } else {
            None
        };
        // A repeat consists solely of its header. Neither independent format can beat it.
        let (wire, independent) = match delta {
            Some(repeat) if repeat.starts_with(b"CR") => (repeat, false),
            // Keep small mesh deltas on their scheduled reference chain without trial-encoding two keys.
            Some(packet)
                if self.mode == Compression::MeshPatch
                    && frame.mesh.is_some()
                    && packet.len() <= frame.raw.len() / 2 =>
            {
                (packet, false)
            }
            delta => {
                let spatial = frame.candidate(None, MeshCoding::Standard)?;
                let mut key =
                    if spatial.len() < frame.raw.len() { spatial } else { frame.raw.clone() };
                if self.mode == Compression::MeshPatch && frame.mesh.is_some() {
                    let dense = frame.candidate(None, MeshCoding::Patch)?;
                    if dense.len() < key.len() {
                        key = dense;
                    }
                }
                match delta {
                    Some(d) if d.len() < key.len() => (d, false),
                    _ => (key, true),
                }
            }
        };
        self.refs.insert(frame, independent);
        Ok(wire)
    }
}

/// Matches the reference's start vertex and winding without changing any polygon edge.
fn align_vertices(encoded: &mut [u8], reference: &[u8]) {
    if encoded == reference {
        return;
    }
    // Nonzero residuals need a change flag and one or two encoded nibbles.
    let cost = |bytes: &[u8]| -> u32 {
        bytes
            .iter()
            .zip(reference)
            .map(|(&a, &b)| a ^ b)
            .filter(|&delta| delta != 0)
            .map(|delta| 4 + delta.count_ones() + u32::from(delta > 15))
            .sum()
    };
    let mut best = cost(encoded);
    let mut order = None;
    for_each_vertex_order(encoded, |candidate| {
        let estimate = cost(candidate);
        if estimate < best {
            order = Some(*candidate);
            best = estimate;
        }
        best != 0
    });
    if let Some(order) = order {
        encoded.copy_from_slice(&order);
    }
}

/// Reuses an identical edge sequence from a nearby reference position when it lowers the cost.
fn align_translated_outline(
    index: usize,
    record: &mut [u8; RECORD],
    references: &[[u8; RECORD]],
    shapes: &HashMap<&[u8], Vec<usize>>,
) {
    let mut best = layered::motion_cost(index, record, references);
    if best < 15 {
        return;
    }
    let low = index.saturating_sub(layered::REFERENCE_RADIUS);
    let high = index + layered::REFERENCE_RADIUS;
    let mut order = None;
    let mut trial = *record;
    for_each_vertex_order(&record[2..], |candidate| {
        let Some(indices) = shapes.get(&candidate[2..]) else { return true };
        trial[2..].copy_from_slice(candidate);
        let start = indices.partition_point(|&position| position < low);
        for &position in indices[start..].iter().take_while(|&&position| position <= high) {
            let estimate = layered::record_cost(&trial, &references[position])
                + layered::REFERENCE_COST * u32::from(position != index);
            if estimate + 3 < best {
                best = estimate;
                order = Some(*candidate);
            }
        }
        best > 3
    });
    if let Some(order) = order {
        record[2..].copy_from_slice(&order);
    }
}

/// Visits cyclic starts and windings of the same closed polygon, stopping when asked.
fn for_each_vertex_order(encoded: &[u8], mut visit: impl FnMut(&[u8; RECORD - 2]) -> bool) {
    let mut points = [[0u8; 2]; (RECORD - 2) / 2];
    let mut count = 0;
    let mut point = [0u8; 2];
    for pair in encoded.chunks_exact(2) {
        point = [point[0].wrapping_add(pair[0]), point[1].wrapping_add(pair[1])];
        if count == 0 || point != points[count - 1] {
            points[count] = point;
            count += 1;
        }
    }
    let mut candidate = [0; RECORD - 2];
    let points = &mut points[..count];
    for reverse in [false, true] {
        if reverse {
            if count <= 2 {
                break;
            }
            points.reverse();
        }
        for start in usize::from(!reverse)..count {
            let mut previous = [0u8; 2];
            for (i, &next) in points[start..].iter().chain(&points[..start]).enumerate() {
                candidate[2 * i] = next[0].wrapping_sub(previous[0]);
                candidate[2 * i + 1] = next[1].wrapping_sub(previous[1]);
                previous = next;
            }
            if !visit(&candidate) {
                return;
            }
        }
    }
}

/// Holds a stream's references across individual Data Track deliveries.
#[derive(Default)]
pub(crate) struct PacketDecoder {
    refs: References,
}

impl PacketDecoder {
    /// Exposes the last reconstructed snapshot for byte-exact benchmark comparisons.
    pub(crate) fn last_snapshot(&self) -> Option<&[u8]> {
        self.refs.last().map(|s| s.raw.as_slice())
    }

    /// Returns no picture for a missing reference or stale packet. Malformed messages never advance state.
    pub(crate) fn decode(&mut self, wire: &[u8]) -> Result<Option<DecodedFrame>> {
        ensure!(wire.len() <= MAX_FRAME_BYTES, "Oversized compressed packet");
        let prefix = wire.get(..2).context("Truncated packet")?;
        let short = short_mesh_magic(wire);
        let magic = short.map_or(prefix, |magic| magic.as_slice());
        let independent = matches!(magic, b"CV" | b"CI" | b"CK");
        let raw = match magic {
            b"CV" => wire.to_vec(),
            b"CI" | b"CK" => {
                let &v = wire.get(2).context("Missing version")?;
                check_size(v, wire.len(), packet_canvas(wire)?)?;
                let (header, encoded) =
                    wire.split_at_checked(header_len(v)).context("Truncated independent header")?;
                let mut raw = header.to_vec();
                raw[1] = b'V';
                unpack_body(
                    &mut raw,
                    encoded,
                    None,
                    if magic == b"CK" { DeltaCodec::MeshPatch } else { DeltaCodec::Standard },
                )?;
                raw
            }
            _ if short.is_some() || matches!(magic, b"CD" | b"CR" | b"CT" | b"CM" | b"CX") => {
                let (index, age, offset) = if short.is_some() {
                    (
                        u32::from_le_bytes(
                            wire.get(1..5).context("Truncated mesh patch index")?.try_into()?,
                        ),
                        if wire[0] & 0x80 == 0 { 1 } else { wire[0] & 63 },
                        5,
                    )
                } else {
                    (
                        u32::from_le_bytes(
                            wire.get(2..6).context("Truncated delta index")?.try_into()?,
                        ),
                        *wire.get(6).context("Truncated reference distance")?,
                        7,
                    )
                };
                ensure!((1..=60).contains(&age), "Invalid reference distance");
                let wanted = index.wrapping_sub(u32::from(age));
                let Some(base) = [self.refs.last(), self.refs.key.as_ref()]
                    .into_iter()
                    .flatten()
                    .find(|r| r.index == wanted)
                else {
                    return Ok(None);
                };
                check_size(base.raw[2], wire.len(), packet_canvas(&base.raw)?)?;
                let mut raw = base.raw[..base.header].to_vec();
                raw[3..7].copy_from_slice(&index.to_le_bytes());
                if short.is_some() {
                    ensure!(base.mesh.is_some(), "Expected mesh patch reference");
                }
                if magic == b"CP" {
                    let mesh = base.mesh.as_ref().context("Expected mesh paint reference")?;
                    raw.extend_from_slice(base.body());
                    mesh.apply_paint(&wire[offset..], &mut raw[base.header..])?;
                } else if magic == b"CR" {
                    ensure!(wire.len() == offset, "Unexpected repeat payload");
                    raw.extend_from_slice(base.body());
                } else {
                    let codec = match magic {
                        b"CT" => DeltaCodec::Compact,
                        b"CM" => DeltaCodec::Motion,
                        b"CX" => DeltaCodec::MeshPatch,
                        _ => DeltaCodec::Standard,
                    };
                    unpack_body(&mut raw, &wire[offset..], Some(base), codec)?;
                }
                raw
            }
            _ => anyhow::bail!("Unsupported vector packet"),
        };
        let batch = DecodedFrame::decode(&raw)?;
        if self.refs.last().is_some_and(|last| {
            let diff = batch.start.wrapping_sub(last.index);
            diff == 0 || diff >= 1 << 31
        }) {
            return Ok(None);
        }
        self.refs.insert(Snapshot::new(raw)?, independent);
        Ok(Some(batch))
    }
}

/// Maps a short mesh header to the equivalent decoder operation.
pub(super) fn short_mesh_magic(wire: &[u8]) -> Option<&'static [u8; 2]> {
    match *wire.first()? {
        b'D' => Some(b"CD"),
        b'T' => Some(b"CT"),
        b'X' => Some(b"CX"),
        0x80..=0xbf => Some(b"CP"),
        0xc0..=0xff => Some(b"CR"),
        _ => None,
    }
}

/// Reconstructs a bounded standard body; the normal CV parser then validates its geometry.
fn unpack_body(
    raw: &mut Vec<u8>,
    mut encoded: &[u8],
    base: Option<&Snapshot>,
    codec: DeltaCodec,
) -> Result<()> {
    let version = raw[2] & 0x7f;
    let format = WireFormat::try_from(version)?;
    let compact = !matches!(codec, DeltaCodec::Standard);
    if format == WireFormat::ColoredPolygon {
        ensure!(!matches!(codec, DeltaCodec::Motion), "Expected a layered motion delta");
        let previous = base.and_then(|r| r.mesh.as_ref());
        let grid = if let Some(m) = previous {
            m.grid
        } else {
            Grid::read(&mut encoded, packet_canvas(raw)?)?
        };
        let mut reader = Reader::new(encoded, compact)?;
        let coding = match codec {
            DeltaCodec::MeshPatch => MeshCoding::Patch,
            DeltaCodec::Compact => MeshCoding::Compact,
            _ => MeshCoding::Standard,
        };
        let mesh = Mesh::predict(&mut reader, grid, None, previous, coding)?;
        reader.finish()?;
        mesh.write(raw);
    } else {
        let (length, data) = encoded.split_first_chunk::<2>().context("Missing byte count")?;
        let len = usize::from(u16::from_le_bytes(*length));
        check_size(raw[2], len + raw.len(), packet_canvas(raw)?)?;
        let mut reader = Reader::new(data, compact)?;
        let reference = base.map(Snapshot::body);
        let body = match codec {
            DeltaCodec::MeshPatch => anyhow::bail!("Expected a colored mesh"),
            DeltaCodec::Standard => bytes(&mut reader, len, &[], reference)?,
            DeltaCodec::Compact => layered::predict::<false>(
                &mut reader,
                len,
                &[],
                reference.context("Missing compact layer reference")?,
            )?,
            DeltaCodec::Motion => layered::predict::<true>(
                &mut reader,
                len,
                &[],
                reference.context("Missing motion layer reference")?,
            )?,
        };
        reader.finish()?;
        raw.extend(body);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rotating or reversing an unchanged outline preserves its raster and sends a repeat.
    #[test]
    fn aligns_layered_outline_to_the_prediction_reference() -> Result<()> {
        let rectangle = |index: u32, first: usize, reversed: bool| {
            let mut packet = b"CV\x87".to_vec();
            packet.extend(index.to_le_bytes());
            packet.extend([30, 0, 1, 0, 1, 0x80, 0x07, 0x38, 0x04]);
            packet.extend([0, 0, 255, 255]);
            let points = [[8u8, 10], [80, 10], [80, 55], [8, 55]];
            let mut last = [0u8; 2];
            for offset in 0..4 {
                let offset = if reversed { 4 - offset } else { offset };
                let next = points[(first + offset) % 4];
                packet.extend([next[0].wrapping_sub(last[0]), next[1].wrapping_sub(last[1])]);
                last = next;
            }
            packet.extend([0; 8]);
            packet
        };
        let mut sender = PacketEncoder::new(Compression::Predictive);
        let mut receiver = PacketDecoder::default();
        let initial = sender.pack(rectangle(0, 0, false))?;
        receiver.decode(&initial)?;
        for (index, reversed) in [(1, false), (2, true)] {
            let mut snapshot = rectangle(index, 2, reversed);
            let expected = DecodedFrame::decode(&snapshot)?.render();
            sender.stabilize_outline_order(&mut snapshot);
            assert_eq!(snapshot, rectangle(index, 0, false));
            let packed = sender.pack(snapshot.clone())?;
            assert_eq!(&packed[..2], b"CR");
            let rendered = receiver.decode(&packed)?.context("Missing repeat")?.render();
            assert_eq!(rendered, expected);
            assert_eq!(receiver.last_snapshot(), Some(snapshot.as_slice()));
        }
        Ok(())
    }

    /// Compact layers preserve exact records and recover when an anchor-based delta arrives.
    #[test]
    fn compact_layers_round_trip_after_a_dropped_delta() -> Result<()> {
        let picture = |index: u32, count: u8| {
            let mut raw = b"CV\x07".to_vec();
            raw.extend(index.to_le_bytes());
            raw.extend([30, 0, 1, 0, 1]);
            raw.extend((index as u16).wrapping_add(32).to_le_bytes());
            for layer in 0..count {
                let fill = 0x783f_u16.wrapping_add(u16::from(layer)).wrapping_add(index as u16);
                raw.extend(fill.to_le_bytes());
                raw.extend([5 + layer, index as u8, 15, 0, 0, 10, 241, 0]);
                raw.extend([0; 8]);
            }
            Snapshot::new(raw)
        };
        let anchor = picture(u32::MAX, 2)?;
        let dropped = picture(0, 3)?;
        let dependent = picture(1, 1)?;
        let recovery = picture(3, 4)?;
        let mut decoder = PacketDecoder::default();
        decoder.decode(&anchor.raw)?.context("Missing anchor")?;
        let missed = dependent.candidate(Some(&dropped), MeshCoding::Compact)?;
        assert!(decoder.decode(&missed)?.is_none());
        let packet = recovery.candidate(Some(&anchor), MeshCoding::Compact)?;
        assert_eq!(&packet[..2], b"CT");
        assert!(decoder.decode(&packet[..packet.len() - 1]).is_err());
        assert_eq!(decoder.last_snapshot(), Some(anchor.raw.as_slice()));
        assert!(decoder.decode(&packet)?.is_some());
        assert_eq!(decoder.last_snapshot(), Some(recovery.raw.as_slice()));
        let smaller = picture(4, 1)?;
        let packet = smaller.candidate(Some(&recovery), MeshCoding::Compact)?;
        assert!(decoder.decode(&packet)?.is_some());
        assert_eq!(decoder.last_snapshot(), Some(smaller.raw.as_slice()));
        Ok(())
    }

    /// Creates reproducible layered pictures with records that can move independently.
    fn layered_picture(index: u32, records: &[[u8; RECORD]]) -> Result<Snapshot> {
        let mut raw = b"CV\x07".to_vec();
        raw.extend_from_slice(&index.to_le_bytes());
        raw.extend_from_slice(&[30, 0, 1, 0, 1, 71, 19]);
        for record in records {
            raw.extend_from_slice(record);
        }
        Snapshot::new(raw)
    }

    /// Generates distinct records for testing without a random-number dependency.
    fn layer_records(count: usize, mut seed: u32) -> Vec<[u8; RECORD]> {
        (0..count)
            .map(|_| {
                std::array::from_fn(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 24) as u8
                })
            })
            .collect()
    }

    /// Reorients a translated shape against a nearby reference while preserving its visible fill.
    #[test]
    fn aligns_translated_outline_after_reordering_layers() -> Result<()> {
        let contour = [[22u8, 25], [80, 27], [85, 39], [67, 74], [21, 48]];
        let record = |first: usize, shift: [u8; 2]| {
            let mut bytes = [0u8; RECORD];
            bytes[..2].copy_from_slice(&0xf813u16.to_le_bytes());
            let mut last = [0u8; 2];
            for (i, pair) in bytes[2..].chunks_exact_mut(2).take(contour.len()).enumerate() {
                let [x, y] = contour[(first + i) % contour.len()];
                let next = [x + shift[0], y + shift[1]];
                pair[0] = next[0].wrapping_sub(last[0]);
                pair[1] = next[1].wrapping_sub(last[1]);
                last = next;
            }
            bytes
        };
        let mut layers = layer_records(26, 231);
        layers[3] = record(0, [0, 0]);
        let anchor = layered_picture(0, &layers)?;
        layers.swap(3, 6);
        layers[6] = record(2, [2, 3]);
        let original = layered_picture(1, &layers)?;
        let mut sender = PacketEncoder::new(Compression::CompactMotion);
        let mut decoder = PacketDecoder::default();
        decoder.decode(&sender.pack(anchor.raw.clone())?)?;
        let mut snapshot = original.raw.clone();
        sender.stabilize_outline_order(&mut snapshot);
        let prepared = Snapshot::new(snapshot.clone())?;
        assert_eq!(prepared.body()[2..].as_chunks::<RECORD>().0[6], record(0, [2, 3]));
        assert!(
            prepared.motion_candidate(&anchor)?.len() < original.motion_candidate(&anchor)?.len()
        );
        let frame = decoder.decode(&sender.pack(snapshot.clone())?)?.context("Missing frame")?;
        assert_eq!(frame.render(), DecodedFrame::decode(&original.raw)?.render());
        assert_eq!(decoder.last_snapshot(), Some(snapshot.as_slice()));
        Ok(())
    }

    /// Relocated and translated polygons round-trip after an anchor-based recovery.
    #[test]
    fn motion_layers_reuse_shapes_in_new_drawing_positions() -> Result<()> {
        let records = layer_records(35, 49);
        let anchor = layered_picture(0, &records)?;
        let mut moved = records;
        moved[..17].rotate_left(1);
        moved[17..].rotate_right(1);
        for record in moved.iter_mut().step_by(4) {
            record[2] = record[2].wrapping_add(4);
            record[3] = record[3].wrapping_sub(3);
        }
        let mut sender = PacketEncoder::new(Compression::CompactMotion);
        let mut decoder = PacketDecoder::default();
        decoder.decode(&sender.pack(anchor.raw.clone())?)?.context("Missing anchor")?;
        sender.pack(layered_picture(1, &moved)?.raw)?;
        moved[2][0] = moved[2][0].wrapping_add(4);
        let dependent = sender.pack(layered_picture(2, &moved)?.raw)?;
        assert!(decoder.decode(&dependent)?.is_none());

        moved.pop();
        let recovered = layered_picture(4, &moved)?;
        let packet = sender.pack(recovered.raw.clone())?;
        assert_eq!(&packet[..2], b"CM");
        assert!(packet.len() < recovered.candidate(Some(&anchor), MeshCoding::Compact)?.len());
        assert!(decoder.decode(&packet[..packet.len() - 1]).is_err());
        assert_eq!(decoder.last_snapshot(), Some(anchor.raw.as_slice()));
        assert!(decoder.decode(&packet)?.is_some());
        assert_eq!(decoder.last_snapshot(), Some(recovered.raw.as_slice()));
        Ok(())
    }

    /// A fragmented motion delta keeps its packet index and exact layer records.
    #[test]
    fn fragmented_motion_layers_round_trip() -> Result<()> {
        let anchor = layered_picture(0, &layer_records(1600, 43))?;
        let target = layered_picture(1, &layer_records(1600, 67))?;
        let parts = crate::messages::split(1, target.motion_candidate(&anchor)?);
        assert!(parts.len() > 1);
        let mut receiver = crate::messages::Receiver::default();
        for part in crate::messages::split(0, anchor.raw) {
            receiver.decode(&part)?;
        }
        let mut decoded = None;
        for part in parts {
            decoded = receiver.decode(&part)?;
        }
        assert_eq!(decoded.context("Missing motion picture")?.start, 1);
        assert_eq!(receiver.last_snapshot(), Some(target.raw.as_slice()));
        Ok(())
    }

    /// Builds an indexed mesh with a controlled number of changed face colors.
    fn frame(index: u32, phase: u8) -> Vec<u8> {
        let mut p = b"CV".to_vec();
        p.push(6);
        p.extend_from_slice(&index.to_le_bytes());
        p.extend_from_slice(&[15, 0, 1, 0, 1, 7]);
        p.extend_from_slice(&[0x77; 104]);
        for i in 0..16u16 {
            p.extend_from_slice(&(i * 0x1081).to_le_bytes());
        }
        for i in 0..84u8 {
            p.push(i.wrapping_mul(67).wrapping_add(u8::from(i == 24) * phase));
        }
        p
    }

    /// Color-only edits and repeats preserve split corners across wrap, loss and stale delivery.
    #[test]
    fn paint_updates_recover_without_changing_the_mesh() -> Result<()> {
        let picture = |index: u32, phase: u8| {
            let mut raw = frame(index, phase);
            let mut mask = [0; 21];
            mask[0] = 1;
            mask[20] = 128;
            raw.extend(mask);
            raw.extend([0x12u8.wrapping_add(phase), 0x34, 0x56u8.wrapping_sub(phase)]);
            raw
        };
        let mut sender = PacketEncoder::new(Compression::MeshPatch);
        let mut receiver = PacketDecoder::default();
        let mut late = PacketDecoder::default();
        let anchor = picture(u32::MAX, 0);
        receiver.decode(&sender.pack(anchor.clone())?)?.context("Missing anchor")?;
        let dropped = sender.pack(picture(0, 1))?;
        assert_eq!(dropped[0], 0x81);
        assert!(late.decode(&dropped)?.is_none());
        for end in 0..dropped.len() {
            assert!(receiver.decode(&dropped[..end]).is_err());
        }
        let mut bad = dropped.clone();
        bad.push(0);
        assert!(receiver.decode(&bad).is_err());
        for invalid in [&[0][..], &[0xff, 0xff, 0x7f], &[1, 0, 0, 0, 0]] {
            let mut bad = dropped[..5].to_vec();
            bad.extend(invalid);
            assert!(receiver.decode(&bad).is_err());
        }
        assert_eq!(receiver.last_snapshot(), Some(anchor.as_slice()));
        for i in 1..3 {
            assert!(receiver.decode(&sender.pack(picture(i, i as u8 + 1))?)?.is_none());
        }
        let recovered = picture(3, 4);
        let packet = sender.pack(recovered.clone())?;
        assert_eq!(packet[0], 0x84);
        receiver.decode(&packet)?.context("Missing anchor-based patch")?;
        assert!(receiver.decode(&dropped)?.is_none());
        assert_eq!(receiver.last_snapshot(), Some(recovered.as_slice()));
        let repeat = sender.pack(picture(4, 4))?;
        assert_eq!((repeat[0], repeat.len()), (0xc1, 5));
        let mut invalid = repeat.clone();
        invalid.push(0);
        assert!(receiver.decode(&invalid).is_err());
        assert_eq!(receiver.decode(&repeat)?.context("Missing repeat")?.start, 4);
        sender.reset();
        let new_key = sender.pack(picture(5, 4))?;
        assert!(late.decode(&new_key)?.is_some());
        Ok(())
    }

    /// All palette widths, multibyte edit counts and both vertex precisions restore exact CV bytes.
    #[test]
    fn mesh_patch_palette_sizes_and_dense_refreshes_round_trip() -> Result<()> {
        for size in 2..=16u8 {
            let mut raw = frame(20, 0);
            let palette = 13 + 104;
            let faces = palette + 32;
            for i in usize::from(size)..16 {
                raw[palette + 2 * i..palette + 2 * i + 2].fill(0);
            }
            raw[faces..].fill(0);
            let base = Snapshot::new(raw)?;
            let mut target = base.raw.clone();
            target[3..7].copy_from_slice(&21u32.to_le_bytes());
            for (i, pair) in target[faces..].iter_mut().enumerate() {
                let color = |i: usize| 1 + (i % usize::from(size - 1)) as u8;
                *pair = color(2 * i) << 4 | color(2 * i + 1);
            }
            let next = Snapshot::new(target)?;
            let patch = next.paint_candidate(&base).context("Expected a paint patch")?;
            assert_eq!(patch[5..7], [0xa8, 1]); // 168 changed base triangles
            let mut receiver = PacketDecoder::default();
            let key = base.candidate(None, MeshCoding::Patch)?;
            receiver.decode(&key)?.context("Missing dense anchor")?;
            receiver.decode(&patch)?.context("Missing color update")?;
            assert_eq!(receiver.last_snapshot(), Some(next.raw.as_slice()));
            for offset in [0x97, 0x87] {
                let mut dense = base.raw.clone();
                dense[3..7].copy_from_slice(&22u32.to_le_bytes());
                dense[13 + 14] = offset;
                dense[palette..palette + 2].copy_from_slice(&0x8642u16.to_le_bytes());
                let dense = Snapshot::new(dense)?;
                let packet = dense.candidate(Some(&base), MeshCoding::Patch)?;
                let mut receiver = PacketDecoder::default();
                receiver.decode(&key)?;
                assert!(receiver.decode(&packet[..packet.len() - 1]).is_err());
                receiver.decode(&packet)?.context("Missing mesh refresh")?;
                assert_eq!(receiver.last_snapshot(), Some(dense.raw.as_slice()));
            }
        }
        Ok(())
    }

    /// Consecutive dense headers use the full source index across wrap and require their exact base.
    #[test]
    fn short_dense_mesh_headers_use_the_preceding_frame() -> Result<()> {
        let base = Snapshot::new(frame(u32::MAX, 0))?;
        let next = Snapshot::new(frame(0, 4))?;
        for coding in [MeshCoding::Standard, MeshCoding::Compact, MeshCoding::Patch] {
            let delta = next.candidate(Some(&base), coding)?;
            let mut short = vec![delta[1]];
            short.extend(&delta[2..6]);
            short.extend(&delta[7..]);
            let mut receiver = PacketDecoder::default();
            assert!(receiver.decode(&short)?.is_none());
            receiver.decode(&base.raw)?;
            assert!(receiver.decode(&short[..short.len() - 1]).is_err());
            receiver.decode(&short)?.context("Missing consecutive mesh delta")?;
            assert_eq!(receiver.last_snapshot(), Some(next.raw.as_slice()));
            assert!(receiver.decode(&short)?.is_none());
        }
        Ok(())
    }

    /// Reassembly validates the absolute frame index before applying a large paint patch.
    #[test]
    fn reassembles_out_of_order_mesh_paint_fragments() -> Result<()> {
        let mut raw = b"CV\x86".to_vec();
        raw.extend(100u32.to_le_bytes());
        raw.extend([30, 0, 1, 0, 1]);
        raw.extend(1920u16.to_le_bytes());
        raw.extend(1080u16.to_le_bytes());
        raw.extend([255, 160, 0, 120, 0]);
        raw.resize(raw.len() + 161 * 121, 0x77);
        for i in 0..16u16 {
            raw.extend((i * 0x1081).to_le_bytes());
        }
        let colors = raw.len();
        raw.resize(colors + 160 * 120, 0);
        let anchor = Snapshot::new(raw)?;
        let mut raw = anchor.raw.clone();
        raw[3..7].copy_from_slice(&101u32.to_le_bytes());
        raw[colors..].fill(0x12);
        let next = Snapshot::new(raw)?;
        let patch = next.paint_candidate(&anchor).context("Missing mesh patch")?;
        let mut receiver = crate::messages::Receiver::default();
        for part in crate::messages::split(100, anchor.raw.clone()) {
            receiver.decode(&part)?;
        }
        let wrong: Vec<_> = crate::messages::split(102, patch.clone()).collect();
        for part in &wrong[..wrong.len() - 1] {
            assert!(receiver.decode(part)?.is_none());
        }
        assert!(receiver.decode(wrong.last().expect("has fragments")).is_err());
        assert_eq!(receiver.last_snapshot(), Some(anchor.raw.as_slice()));
        let packets: Vec<_> = crate::messages::split(101, patch).collect();
        assert!(packets.len() > 1);
        for part in packets.iter().skip(1).rev() {
            assert!(receiver.decode(part)?.is_none());
        }
        assert_eq!(receiver.decode(&packets[0])?.context("Missing reassembled patch")?.start, 101);
        assert_eq!(receiver.last_snapshot(), Some(next.raw.as_slice()));
        Ok(())
    }

    /// Subdivisions, moved vertices, RGB565 palette changes and child colors round-trip exactly.
    #[test]
    fn predicts_refined_triangles() {
        let mut sender = PacketEncoder::new(Compression::Predictive);
        let mut receiver = PacketDecoder::default();
        for i in 0..4u8 {
            let mut raw = frame(u32::from(i), i);
            raw[13 + 14] = 0x77 + i;
            raw[13 + 104] = i;
            let mut mask = [0; 21]; // 168 base faces on grid 7
            mask[2] = 1 << i;
            raw.extend_from_slice(&mask);
            raw.extend_from_slice(&[0x12 + i, 0x30]);
            let packed = sender.pack(raw.clone()).unwrap();
            if i > 0 {
                assert_eq!(&packed[..2], b"CD");
            }
            let batch = receiver.decode(&packed).unwrap().unwrap();
            assert_eq!(receiver.last_snapshot(), Some(raw.as_slice()));
            assert_eq!(batch.render(), DecodedFrame::decode(&raw).unwrap().render());
        }
    }

    /// Compact deltas retain, change and remove child faces across index wrap without decoder drift.
    #[test]
    fn compact_face_reuse_round_trips() {
        let mut old = Snapshot::new(frame(u32::MAX, 0)).unwrap();
        let mut decoder = PacketDecoder::default();
        decoder.decode(&old.raw).unwrap().unwrap();
        for (index, split) in [true, true, false].into_iter().enumerate() {
            let mut raw = frame(index as u32, index as u8 + 1);
            if split {
                let mut mask = [0; 21];
                mask[0] = 3;
                raw.extend_from_slice(&mask);
                raw.extend_from_slice(&[0x12, 0x34, 0x56]); // six child color indices
            }
            let next = Snapshot::new(raw).unwrap();
            let delta = next.candidate(Some(&old), MeshCoding::Compact).unwrap();
            assert_eq!(&delta[..2], b"CT");
            assert!(PacketDecoder::default().decode(&delta).unwrap().is_none());
            assert!(decoder.decode(&delta[..delta.len() - 1]).is_err());
            assert_eq!(decoder.last_snapshot(), Some(old.raw.as_slice()));
            decoder.decode(&delta).unwrap().unwrap();
            assert_eq!(decoder.last_snapshot(), Some(next.raw.as_slice()));
            assert!(decoder.decode(&delta).unwrap().is_none());
            old = next;
        }
    }

    /// Dense keyframes and temporal updates restore the explicit grid and the original snapshot.
    #[test]
    fn dense_grid_round_trips_beyond_legacy_size() {
        let mut raw = b"CV\x86".to_vec();
        raw.extend_from_slice(&0u32.to_le_bytes());
        raw.extend_from_slice(&[30, 0, 1, 0, 1]);
        for v in [1280u16, 720] {
            raw.extend_from_slice(&v.to_le_bytes());
        }
        raw.extend_from_slice(&[255, 60, 0, 34, 0]);
        raw.resize(raw.len() + 61 * 35, 0x77);
        raw.resize(raw.len() + 32 + 60 * 34, 0);
        let mut sender = PacketEncoder::new(Compression::Predictive);
        let mut decoder = PacketDecoder::default();
        for index in 0..3u32 {
            raw[3..7].copy_from_slice(&index.to_le_bytes());
            *raw.last_mut().unwrap() = index as u8;
            let wire = sender.pack(raw.clone()).unwrap();
            assert_eq!(&wire[..2], if index == 0 { b"CI" } else { b"CD" });
            assert_eq!(decoder.decode(&wire).unwrap().unwrap().polygon_count(), 4080);
            assert_eq!(decoder.last_snapshot(), Some(raw.as_slice()));
        }
        for magic in *b"IV" {
            raw[1] = magic;
            raw[17..21].fill(0xff);
            assert!(decoder.decode(&raw).is_err());
        }
    }

    /// A dropped delta heals from the anchor. A late join or lost anchor heals at the next refresh.
    #[test]
    fn predictive_loss_reordering_and_late_join() {
        let mut encoder = PacketEncoder::new(Compression::Predictive);
        let mut live = PacketDecoder::default();
        let mut late = PacketDecoder::default();
        let mut saved = Vec::new();
        for i in 0..34 {
            let raw = frame(i, i as u8);
            let packet = encoder.pack(raw.clone()).unwrap();
            assert!(packet.len() <= raw.len());
            if i == 0 || i == 15 || i == 30 {
                assert!(matches!(&packet[..2], b"CI" | b"CV"));
            }
            if i == 1 || i == 15 {
                saved = packet;
                continue;
            }
            let decoded = live.decode(&packet).unwrap();
            if (2..4).contains(&i) || (16..30).contains(&i) {
                assert!(decoded.is_none(), "unexpected dependent picture {i}");
            } else {
                assert_eq!(decoded.unwrap().start, i);
                assert_eq!(live.last_snapshot(), Some(raw.as_slice()));
            }
            if i == 4 {
                assert!(live.decode(&saved).unwrap().is_none());
            }
            if i >= 5 {
                let decoded = late.decode(&packet).unwrap();
                assert_eq!(decoded.is_some(), i >= 30);
            }
        }
    }

    /// Minimum-bandwidth mesh deltas recover from their anchor after a dropped picture.
    #[test]
    fn low_bandwidth_recovery_keeps_one_second_keyframes() -> Result<()> {
        let mut encoder = PacketEncoder::new(Compression::Compact).with_longer_mesh_recovery();
        let mut live = PacketDecoder::default();
        let mut late_join = PacketDecoder::default();
        for index in 0..=30 {
            let mut raw = frame(index, index as u8);
            raw[7] = 30;
            let packet = encoder.pack(raw.clone())?;
            if index.is_multiple_of(30) {
                assert!(matches!(&packet[..2], b"CI" | b"CV"));
            } else {
                assert!(matches!(&packet[..2], b"CD" | b"CT"));
                assert_eq!(packet[6], if index.is_multiple_of(12) { index as u8 } else { 1 });
            }
            if index > 0 {
                assert_eq!(late_join.decode(&packet)?.is_some(), index == 30);
            }
            if index == 1 || index == 13 {
                continue;
            }
            let decoded = live.decode(&packet)?;
            if (2..12).contains(&index) || (14..24).contains(&index) {
                assert!(decoded.is_none());
            } else {
                assert_eq!(decoded.context("Missing mesh recovery picture")?.start, index);
                assert_eq!(live.last_snapshot(), Some(raw.as_slice()));
            }
        }
        Ok(())
    }

    /// Malformed entropy bodies cannot poison a decoder's temporal references.
    #[test]
    fn damaged_delta_rolls_back_and_repeats_are_explicit() {
        let mut encoder = PacketEncoder::new(Compression::Predictive);
        let mut decoder = PacketDecoder::default();
        let first = frame(u32::MAX, 1);
        let key = encoder.pack(first.clone()).unwrap();
        decoder.decode(&key).unwrap().unwrap();
        let second = frame(0, 2);
        let delta = encoder.pack(second.clone()).unwrap();
        assert_eq!(&delta[..2], b"CD");
        for end in 0..delta.len() {
            assert!(decoder.decode(&delta[..end]).is_err(), "accepted truncation at {end}");
            assert_eq!(decoder.last_snapshot(), Some(first.as_slice()));
        }
        let mut invalid = delta.clone();
        invalid.push(12);
        assert!(decoder.decode(&invalid).is_err());
        assert!(decoder.decode(&delta).unwrap().is_some());
        assert_eq!(decoder.last_snapshot(), Some(second.as_slice()));
        let repeat = encoder.pack(frame(1, 2)).unwrap();
        assert_eq!(repeat.len(), 7);
        assert_eq!(&repeat[..2], b"CR");
        assert_eq!(decoder.decode(&repeat).unwrap().unwrap().start, 1);
        encoder.reset();
        let key = encoder.pack(frame(2, 2)).unwrap();
        assert!(matches!(&key[..2], b"CV" | b"CI"));
    }
}
