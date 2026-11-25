//! Lossless temporal prediction for an ordered list of polygon layers.
use anyhow::{ensure, Context, Result};

use super::entropy::{rgb565, Field, Symbols};
use crate::layers::RECORD;

const REFERENCE_BITS: u8 = 4;
/// Maximum distance between the drawing positions of a layer and its motion reference.
pub(super) const REFERENCE_RADIUS: usize = 1 << (REFERENCE_BITS - 1);
/// Estimated bit cost of referring to another position in the reference picture.
pub(super) const REFERENCE_COST: u32 = 5;
const EMPTY: [u8; RECORD] = [0; RECORD];

/// Predicts the fills and vertex offsets against the reference display list.
pub(super) fn predict<const MOTION: bool>(
    io: &mut impl Symbols,
    len: usize,
    target: &[u8],
    reference: &[u8],
) -> Result<Vec<u8>> {
    ensure!(len >= 2 && (len - 2).is_multiple_of(RECORD), "Invalid layer record count");
    let mut decoded = Vec::with_capacity(len);
    let fill = |row: &[u8]| u16::from_le_bytes(row.first_chunk::<2>().copied().unwrap_or_default());
    decoded.extend_from_slice(&rgb565(io, fill(target), fill(reference))?.to_le_bytes());
    let target = target.get(2..).unwrap_or_default().as_chunks::<RECORD>().0;
    let reference = reference.get(2..).unwrap_or_default().as_chunks::<RECORD>().0;
    for layer in 0..(len - 2) / RECORD {
        let next = target.get(layer).unwrap_or(&EMPTY);
        let index =
            if MOTION { motion_reference(io, layer, target.get(layer), reference)? } else { layer };
        let base = reference.get(index).unwrap_or(&EMPTY);
        if !io.flag(Field::LayerChanged, next != base)? {
            decoded.extend_from_slice(base);
            continue;
        }
        let color = if MOTION && io.flag(Field::SameLayerFill, next[..2] == base[..2])? {
            fill(base)
        } else {
            rgb565(io, fill(next), fill(base))?
        };
        decoded.extend_from_slice(&color.to_le_bytes());
        // All coordinate pairs after the first are edge offsets, which survive translation.
        let same_shape = MOTION && io.flag(Field::SameLayerShape, next[4..] == base[4..])?;
        let end = if same_shape { 4 } else { RECORD };
        for (next, base) in next[2..end].chunks_exact(2).zip(base[2..end].chunks_exact(2)) {
            if io.flag(Field::VertexMoved, next != base)? {
                decoded.push(coordinate(io, Field::VertexX, next[0], base[0])?);
                decoded.push(coordinate(io, Field::VertexY, next[1], base[1])?);
            } else {
                decoded.extend_from_slice(base);
            }
        }
        if same_shape {
            decoded.extend_from_slice(&base[end..]);
        }
    }
    Ok(decoded)
}

/// Codes a bounded reference index without changing the current frame's painting order.
fn motion_reference(
    io: &mut impl Symbols,
    layer: usize,
    target: Option<&[u8; RECORD]>,
    reference: &[[u8; RECORD]],
) -> Result<usize> {
    let alternate = target.and_then(|record| closest_reference(layer, record, reference));
    if !io.flag(Field::LayerReference, alternate.is_some())? {
        return Ok(layer);
    }
    let rank = alternate
        .map_or(0, |index| 2 * (index.abs_diff(layer) as u8 - 1) + u8::from(index < layer));
    let rank = io.number(Field::LayerReferenceOffset, REFERENCE_BITS, rank)?;
    let distance = usize::from(rank / 2) + 1;
    let index =
        if rank & 1 == 0 { layer.checked_add(distance) } else { layer.checked_sub(distance) };
    index.filter(|&i| i < reference.len()).context("Invalid alternate layer reference")
}

/// Finds a nearby reference only when its estimated savings pay for the reference index.
fn closest_reference(
    layer: usize,
    target: &[u8; RECORD],
    reference: &[[u8; RECORD]],
) -> Option<usize> {
    let mut cost =
        record_cost(target, reference.get(layer).unwrap_or(&EMPTY)).saturating_sub(REFERENCE_COST);
    if cost <= 1 {
        return None;
    }
    let mut best = None;
    let start = layer.saturating_sub(REFERENCE_RADIUS);
    let end = (layer + REFERENCE_RADIUS + 1).min(reference.len());
    for (index, reference) in reference.iter().enumerate().take(end).skip(start) {
        if index == layer {
            continue;
        }
        let candidate = record_cost(target, reference);
        if candidate < cost {
            best = Some(index);
            cost = candidate;
        }
    }
    best
}

/// Estimates the cost that the motion predictor would select for this record.
pub(super) fn motion_cost(layer: usize, target: &[u8; RECORD], reference: &[[u8; RECORD]]) -> u32 {
    closest_reference(layer, target, reference).map_or_else(
        || record_cost(target, reference.get(layer).unwrap_or(&EMPTY)),
        |index| REFERENCE_COST + record_cost(target, &reference[index]),
    )
}

/// Estimates how many bits a predictor would spend on a record's residuals.
pub(super) fn record_cost(target: &[u8; RECORD], base: &[u8; RECORD]) -> u32 {
    if target == base {
        return 0;
    }
    let color = (target[0] ^ base[0]).count_ones() + (target[1] ^ base[1]).count_ones();
    let coordinates: u32 = target[2..]
        .iter()
        .zip(&base[2..])
        .map(|(&a, &b)| match (a.wrapping_sub(b) as i8).unsigned_abs() {
            0 => 0,
            1 => 2,
            2..=4 => 3,
            5..=7 => 4,
            8..=15 => 6,
            _ => 9,
        })
        .sum();
    1 + color + coordinates
}

/// Favors small signed changes to a vertex coordinate or to an edge offset.
fn coordinate(io: &mut impl Symbols, axis: Field, value: u8, prediction: u8) -> Result<u8> {
    let delta = value.wrapping_sub(prediction);
    let rank = (delta << 1) ^ 0u8.wrapping_sub(delta >> 7);
    let rank = if io.flag(Field::WideCoordinate, rank >= 16)? {
        let high = io.number(Field::ByteHigh, 4, rank >> 4)?;
        let low = io.number(Field::ByteLow, 4, rank & 15)?;
        (high << 4) | low
    } else {
        io.number(axis, 4, rank)?
    };
    Ok(prediction.wrapping_add((rank >> 1) ^ 0u8.wrapping_sub(rank & 1)))
}
