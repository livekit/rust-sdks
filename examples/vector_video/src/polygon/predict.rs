//! Lossless prediction over stable vertex, palette, face and child identities.
use super::*;
use crate::compression::entropy::{component, rgb565, Field, Symbols};

/// Selects the lossless field representation carried by the packet.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeshCoding {
    Standard,
    Compact,
    Patch,
}

/// Codes palette changes using green as the brightness predictor for red and blue.
fn patch_color(io: &mut impl Symbols, value: u16, old: u16) -> Result<u16> {
    if !io.flag(Field::ColorChanged, value != old)? {
        return Ok(old);
    }
    let old_green = ((old >> 5) & 63) as u8;
    let green = component(io, Field::Green, 6, ((value >> 5) & 63) as u8, old_green)?;
    let light = (i16::from(green) - i16::from(old_green)) / 2;
    let mut word = u16::from(green) << 5;
    for (field, shift) in [(Field::Red, 11), (Field::Blue, 0)] {
        let prediction = (i16::from(((old >> shift) & 31) as u8) + light).clamp(0, 31) as u8;
        word |=
            u16::from(component(io, field, 5, ((value >> shift) & 31) as u8, prediction)?) << shift;
    }
    Ok(word)
}

/// Predicts a complete RGB565 entry, keeping component magnitudes independent.
fn color(io: &mut impl Symbols, value: u16, prediction: u16) -> Result<u16> {
    if !io.flag(Field::ColorChanged, value != prediction)? {
        return Ok(prediction);
    }
    rgb565(io, value, prediction)
}

/// Codes palette ranks with local predictions first, then colors closest to the leading prediction.
fn index(
    io: &mut impl Symbols,
    field: Field,
    value: u8,
    guesses: &[u8],
    neighbors: &[[u8; 16]; 16],
) -> Result<u8> {
    let mut order = [0; 16];
    let mut seen = 0u16;
    let mut len = 0;
    for &candidate in guesses.iter().chain(&neighbors[usize::from(guesses[0])]) {
        let bit = 1u16 << candidate;
        if seen & bit == 0 {
            order[len] = candidate;
            seen |= bit;
            len += 1;
        }
    }
    let rank = order.iter().position(|&v| v == value).expect("all palette entries are ranked");
    Ok(order[usize::from(io.number(field, 4, rank as u8)?)])
}

impl Mesh {
    /// Writes or reads one mesh using exactly the same predictions and symbol order on both peers.
    pub(crate) fn predict(
        io: &mut impl Symbols,
        grid: Grid,
        target: Option<&Self>,
        reference: Option<&Self>,
        coding: MeshCoding,
    ) -> Result<Self> {
        let (w, h) = grid.dimensions();
        ensure!(reference.is_none_or(|m| m.grid == grid), "Incompatible mesh reference");
        let patch = coding == MeshCoding::Patch;
        let compact = coding == MeshCoding::Compact;
        // A coarse fit often moves every vertex by whole pairs of grid units.
        let even_steps = patch
            && io.flag(
                Field::EvenSteps,
                target.is_some_and(|m| {
                    m.offsets.iter().enumerate().all(|(i, to)| {
                        let old = reference.map_or([0; 2], |r| r.offsets[i]);
                        (0..2).all(|axis| (to[axis] - old[axis]) % 2 == 0)
                    })
                }),
            )?;
        let shift = u8::from(even_steps);
        let mut offsets = Vec::with_capacity(grid.vertices());
        for i in 0..grid.vertices() {
            let old = reference.map_or([0; 2], |m| m.offsets[i]);
            let desired = target.map_or([0; 2], |m| m.offsets[i]);
            let mut offset = old;
            let (x, y) = (i % (w + 1), i / (w + 1));
            let (movex, movey) = (x > 0 && x < w, y > 0 && y < h);
            if (movex || movey) && io.flag(Field::VertexMoved, desired != old)? {
                for (axis, movable, field) in
                    [(0, movex, Field::VertexX), (1, movey, Field::VertexY)]
                {
                    if movable {
                        let prior = (old[axis] + 7) as u8;
                        let encoded = component(
                            io,
                            field,
                            4 - shift,
                            (desired[axis] + 7) as u8 >> shift,
                            prior >> shift,
                        )?;
                        offset[axis] = ((encoded << shift) | (prior & shift)) as i8 - 7;
                    }
                }
            }
            ensure!(grid.allowed(i, offset), "Invalid predicted vertex");
            offsets.push(offset);
        }
        let faces = 2 * w * h;
        if faces * 2 <= 32 + faces / 2 {
            let mut words = Vec::with_capacity(faces);
            for i in 0..faces {
                let pred = reference
                    .map_or_else(|| words.last().copied().unwrap_or(0), |m| m.colors.at(i));
                words.push(color(io, target.map_or(0, |m| m.colors.at(i)), pred)?);
            }
            return Ok(Self {
                grid,
                offsets,
                colors: FaceColors::Direct(words),
                detail: Vec::new(),
            });
        }
        let indexed = |m: &Self, i| match &m.colors {
            FaceColors::Indexed { palette, .. } => palette[i],
            FaceColors::Direct(_) => unreachable!("grid specifies indexed colors"),
        };
        let base = |m: &Self, i| match &m.colors {
            // The raw wire color of a split face is its center child's palette slot.
            FaceColors::Indexed { indices, .. } => indices[i],
            FaceColors::Direct(_) => unreachable!("grid specifies indexed colors"),
        };
        let mut palette = [0; 16];
        let active = if patch {
            let last = target.and_then(Self::palette_prefix_len).unwrap_or(1) - 1;
            usize::from(io.number(Field::PaletteLength, 4, last as u8)?) + 1
        } else {
            16
        };
        for i in 0..active {
            let value = target.map_or(0, |m| indexed(m, i));
            let prior = reference.map_or(0, |m| indexed(m, i));
            palette[i] = if patch {
                let prediction = if reference.is_none() && i > 0 { palette[i - 1] } else { prior };
                patch_color(io, value, prediction)?
            } else {
                color(io, value, prior)?
            };
        }
        let first = palette[0];
        palette[active..].fill(first);
        let rgb = palette.map(unpack);
        let neighbors = std::array::from_fn(|i| {
            let mut order: [u8; 16] = std::array::from_fn(|j| j as u8);
            order.sort_unstable_by_key(|&j| {
                let other = rgb[usize::from(j)];
                ((0..3).map(|k| u32::from(rgb[i][k].abs_diff(other[k])).pow(2)).sum::<u32>(), j)
            });
            order
        });
        let mut indices = Vec::with_capacity(faces);
        let has_detail =
            io.flag(Field::DetailPresent, target.is_some_and(|m| !m.detail.is_empty()))?;
        let mut detail = if has_detail { Vec::with_capacity(faces) } else { Vec::new() };
        let mut vertex_color = vec![None; grid.vertices()];
        for (i, face) in grid.faces().enumerate() {
            let left = indices.last().copied().unwrap_or(0);
            let above = i.checked_sub(2 * w).map_or(left, |j| indices[j]);
            let old = reference.map_or(left, |m| base(m, i));
            let old_children = reference.and_then(|m| m.detail.get(i)).copied().flatten();
            let target_children = target.and_then(|m| m.detail.get(i)).copied().flatten();
            if compact {
                let changed =
                    target.is_some_and(|m| base(m, i) != old || target_children != old_children);
                if !io.flag(Field::FaceChanged, changed)? {
                    ensure!(
                        has_detail || old_children.is_none(),
                        "Unexpected retained subdivision"
                    );
                    indices.push(old);
                    if has_detail {
                        detail.push(old_children);
                    }
                    for (v, color) in face.into_iter().zip(old_children.unwrap_or([old; 4])) {
                        vertex_color[v] = Some(color);
                    }
                    continue;
                }
            }
            let nearby = face.map(|v| vertex_color[v].unwrap_or(left));
            let field =
                if old == left || old == above { Field::FaceAgreement } else { Field::FaceIndex };
            let center = index(
                io,
                field,
                target.map_or(0, |m| base(m, i)),
                &[old, left, above, nearby[0], nearby[1], nearby[2]],
                &neighbors,
            )?;
            indices.push(center);
            let mut children = [center; 4];
            if has_detail {
                let field =
                    if old_children.is_some() { Field::SplitRetained } else { Field::SplitAdded };
                if io.flag(field, target_children.is_some())? {
                    for j in 0..3 {
                        let adjacent = vertex_color[face[j]].unwrap_or(center);
                        let (guess, field) = old_children
                            .map_or((adjacent, Field::CornerIndex), |c| {
                                (c[j], Field::TrackedCorner)
                            });
                        let sibling = if j == 0 { center } else { children[j - 1] };
                        children[j] = index(
                            io,
                            field,
                            target_children.map_or(0, |c| c[j]),
                            &[guess, center, adjacent, old, sibling],
                            &neighbors,
                        )?;
                    }
                    detail.push(Some(children));
                } else {
                    detail.push(None);
                }
            }
            for (v, color) in face.into_iter().zip(children) {
                vertex_color[v] = Some(color);
            }
        }
        ensure!(!has_detail || detail.iter().any(Option::is_some), "Empty predicted split mask");
        Ok(Self { grid, offsets, colors: FaceColors::Indexed { palette, indices }, detail })
    }
}
