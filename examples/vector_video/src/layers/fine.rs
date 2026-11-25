//! Fits compact contours after the broad paint layers using the original fine samples.
mod contrast;
mod motion;

use std::collections::HashSet;

use super::{fit::luma, regions, Edges, Outline, Patch, Scene, DETAIL_PLANE, W};
use crate::polygon::{pack, unpack};

type Color = [i16; 3];

/// Inputs for the fine pass, after the underlying scene has been fitted.
pub(super) struct Input<'a> {
    pub(super) rgb: &'a [[u8; 3]],
    pub(super) signal: &'a [i16],
    pub(super) subject: &'a [bool],
    pub(super) previous: &'a [Patch],
    pub(super) previous_rgb: &'a [[u8; 3]],
    pub(super) slots: usize,
    pub(super) textured: bool,
}

/// A pair of pixels whose contrast changes when a polygon is painted.
struct Edge {
    first: usize,
    second: usize,
    reference: Color,
    interior: bool,
    #[cfg(feature = "object-layers")]
    boundary: bool,
}

/// A compact candidate with exact fine-grid coverage and affected contrast pairs.
struct Candidate {
    patch: Patch,
    fill: Color,
    pixels: Vec<usize>,
    edges: Vec<Edge>,
    weight: i64,
    bounds: [usize; 4],
    on_subject: bool,
    #[cfg(feature = "object-layers")]
    mark: usize,
}

/// Describes luma and two chromatic components for contrasting and comparing colors.
fn components(rgb: [u8; 3]) -> Color {
    let [r, g, b] = rgb.map(i16::from);
    [luma(rgb) as i16, (r - g) / 2, (b - g) / 2]
}

/// Computes a signed color difference for one adjacent or short-range pair.
fn difference(a: Color, b: Color) -> Color {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Gives luma and chroma errors a common scale while favoring recognizable contrast.
fn error(a: Color, b: Color) -> i32 {
    let [y, u, v] = difference(a, b).map(i32::from);
    2 * y * y + u * u + v * v
}

/// Maps a fine-plane pixel to its enclosing coarse-plane sample.
fn coarse_index(i: usize) -> usize {
    let width = DETAIL_PLANE.width;
    i / width / 2 * W + i % width / 2
}

impl Candidate {
    /// Looks up the real colors and gradient pairs served by a quantized polygon.
    fn new(
        outline: Outline,
        input: &Input<'_>,
        reference: &[Color],
        coverage: &mut [bool],
        old_color: Option<u16>,
        #[cfg(feature = "object-layers")] marks: &[u8],
    ) -> Option<Self> {
        let width = DETAIL_PLANE.width;
        let height = DETAIL_PLANE.height;
        let geometry = Edges::new(&outline, DETAIL_PLANE);
        let mut pixels = Vec::new();
        let (mut left, mut right) = (width, 0);
        for y in geometry.top..geometry.bottom {
            geometry.spans(y, |l, r| {
                pixels.extend(y * width + l..y * width + r);
                left = left.min(l);
                right = right.max(r);
            });
        }
        if !(4..=700).contains(&pixels.len()) {
            return None;
        }
        let mut sums = [0u32; 3];
        let mut foreground = 0;
        #[cfg(feature = "object-layers")]
        let mut mark_coverage = [0u16; super::parts::MAX_MARKS + 1];
        for &i in &pixels {
            coverage[i] = true;
            foreground += usize::from(input.subject.get(coarse_index(i)) == Some(&true));
            #[cfg(feature = "object-layers")]
            if let Some(&label) = marks.get(i) {
                mark_coverage[usize::from(label)] += 1;
            }
            for (total, v) in sums.iter_mut().zip(input.rgb[i]) {
                *total += u32::from(v);
            }
        }
        let count = pixels.len() as u32;
        let color = pack(sums.map(|c| ((c + count / 2) / count) as u8));
        let fill = components(unpack(color));
        let color =
            old_color.filter(|&old| error(components(unpack(old)), fill) <= 60).unwrap_or(color);
        let mut edges = Vec::with_capacity(4 * pixels.len());
        for &first in &pixels {
            let (x, y) = (first % width, first / width);
            for distance in [1, 3] {
                let horizontal = [
                    x.checked_sub(distance).map(|_| first - distance),
                    (x + distance < width).then_some(first + distance),
                ];
                let vertical = [
                    y.checked_sub(distance).map(|_| first - distance * width),
                    (y + distance < height).then_some(first + distance * width),
                ];
                for second in horizontal.into_iter().chain(vertical).flatten() {
                    let interior = coverage[second];
                    if !interior || first < second {
                        edges.push(Edge {
                            first,
                            second,
                            reference: difference(reference[first], reference[second]),
                            interior,
                            #[cfg(feature = "object-layers")]
                            boundary: !marks.is_empty()
                                && input.subject.get(coarse_index(first))
                                    != input.subject.get(coarse_index(second)),
                        });
                    }
                }
            }
        }
        for &i in &pixels {
            coverage[i] = false;
        }
        let on_subject = 2 * foreground > pixels.len();
        #[cfg(feature = "object-layers")]
        let mark = mark_coverage
            .iter()
            .enumerate()
            .skip(1)
            .filter(|&(_, &count)| count >= 2)
            .max_by_key(|&(_, &count)| count)
            .map_or(0, |(label, _)| label);
        Some(Self {
            patch: Patch::polygon(outline, color),
            fill: components(unpack(color)),
            weight: 8 + 4 * i64::from(on_subject) + i64::from(old_color.is_some()),
            pixels,
            edges,
            bounds: [left, geometry.top, right, geometry.bottom],
            on_subject,
            #[cfg(feature = "object-layers")]
            mark,
        })
    }

    /// Scores actual changes in edge contrast and penalizes obscuring existing detail.
    fn gain(&self, reference: &[Color], current: &[Color]) -> i64 {
        let mut match_color = 0i64;
        for &i in &self.pixels {
            match_color +=
                i64::from(error(reference[i], current[i]) - error(reference[i], self.fill));
        }
        let mut match_edges = 0i64;
        for edge in &self.edges {
            match_edges += self.edge_gain(edge, current);
        }
        let gain = (match_color + 2 * match_edges) / (self.pixels.len() + 16).isqrt() as i64;
        if gain <= 0 {
            return gain;
        }
        gain * self.weight / 8
    }

    /// Returns the signed improvement to the source contrast on one sampled edge.
    fn edge_gain(&self, edge: &Edge, current: &[Color]) -> i64 {
        let before = difference(current[edge.first], current[edge.second]);
        let after =
            if edge.interior { [0; 3] } else { difference(self.fill, current[edge.second]) };
        i64::from(error(edge.reference, before) - error(edge.reference, after))
    }

    /// Scores the edges the candidate recovers, with a penalty for covering visible edges.
    fn restored_contrast(&self, current: &[Color], include_chroma: bool) -> i64 {
        let mut total = [0i64; 2];
        let mut edges = [0usize; 2];
        let mut restored = [0usize; 2];
        let mut covered = 0i64;
        for edge in &self.edges {
            let [light, u, v] = edge.reference;
            let channel = if include_chroma && light.abs() < 8 {
                if u.abs() >= v.abs() {
                    1
                } else {
                    2
                }
            } else {
                0
            };
            let reference = edge.reference[channel];
            let expected = reference.abs();
            if !(8..=56).contains(&expected) {
                continue;
            }
            let direction = reference.signum();
            let outside = current[edge.second][channel];
            let before = (current[edge.first][channel] - outside) * direction;
            let fraction = |value: i16| {
                let matched = i32::from(value.clamp(0, expected));
                let excess = i32::from((value - expected - 8).clamp(0, expected));
                16 * matched / i32::from(expected) - 12 * excess / i32::from(expected)
            };
            // Painting both endpoints removes any source edge already visible inside the fill.
            if edge.interior {
                if before > 0 {
                    covered += i64::from(fraction(before));
                }
                continue;
            }
            let after = (self.fill[channel] - outside) * direction;
            let change = fraction(after) - fraction(before);
            let kind = usize::from(channel != 0);
            restored[kind] += usize::from(change > 2);
            total[kind] += i64::from(change);
            edges[kind] += 1;
        }
        (0..2)
            .filter(|&kind| restored[kind] >= 8)
            .map(|kind| 64 * (total[kind] - covered / 3).max(0) / (edges[kind] + 24).isqrt() as i64)
            .max()
            .unwrap_or(0)
    }

    /// Adjusts a small fill to reproduce its contrast against the flat layers underneath.
    fn refit_tone(&mut self, reference: &[Color], current: &[Color]) {
        let mut desired = [0i32; 3];
        for &pixel in &self.pixels {
            for (sum, value) in desired.iter_mut().zip(reference[pixel]) {
                *sum += i32::from(value);
            }
        }
        let area = self.pixels.len() as i32;
        let mut weight = area;
        let (mut edge_luma, mut edge_weight) = (0, 0);
        for edge in self.edges.iter().filter(|edge| !edge.interior) {
            let outside = current[edge.second];
            for (sum, (base, contrast)) in
                desired.iter_mut().zip(outside.into_iter().zip(edge.reference))
            {
                *sum += 2 * (i32::from(base) + i32::from(contrast));
            }
            weight += 2;
            if edge.reference[0].abs() >= 8 {
                edge_luma += 2 * (i32::from(outside[0]) + i32::from(edge.reference[0]));
                edge_weight += 2;
            }
        }
        let before = self.fill.map(i32::from);
        // Anchor the adjustment to the source color and leave the rest of the surface flat.
        let offset =
            ((edge_luma - edge_weight * before[0]) / (edge_weight + 2 * area)).clamp(-14, 14);
        let color = pack(
            unpack(self.patch.color)
                .map(|channel| (i32::from(channel) + offset).clamp(0, 255) as u8),
        );
        if color == self.patch.color {
            return;
        }
        let fill = components(unpack(color));
        let after = fill.map(i32::from);
        // Compare quantized colors under the same pixel and edge error as the greedy fit.
        let improvement = (0..3)
            .map(|c| {
                let shift = after[c] - before[c];
                (1 + i32::from(c == 0)) * shift * (2 * desired[c] - weight * (after[c] + before[c]))
            })
            .sum::<i32>();
        if improvement > 8 * weight {
            self.patch.color = color;
            self.fill = fill;
        }
    }

    /// Scores a guided mark while protecting strong contrast along the subject boundary.
    #[cfg(feature = "object-layers")]
    fn guided_gain(&self, reference: &[Color], current: &[Color], marks: &[u8]) -> i64 {
        let label = self.mark as u8;
        let mut local = 0;
        let mut count = 0usize;
        for &i in &self.pixels {
            if marks[i] == label {
                local +=
                    i64::from(error(reference[i], current[i]) - error(reference[i], self.fill));
                count += 1;
            }
        }
        let mut boundary = 0;
        for edge in &self.edges {
            let touches_mark = marks[edge.first] == label || marks[edge.second] == label;
            let strong_boundary = edge.boundary && edge.reference[0].abs() >= 14;
            if touches_mark || strong_boundary {
                let gain = self.edge_gain(edge, current);
                local += if touches_mark { 2 * gain } else { 0 };
                boundary += if strong_boundary { gain } else { 0 };
            }
        }
        if boundary < 0 {
            0
        } else {
            local / (count + 16).isqrt() as i64
        }
    }

    /// Checks whether a repaint can change a sampled pixel or one of its contrast neighbors.
    fn affected(&self, [left, top, right, bottom]: [usize; 4]) -> bool {
        let [l, t, r, b] = self.bounds;
        l < right + 3 && left < r + 3 && t < bottom + 3 && top < b + 3
    }
}

/// Measures local brightness relative to a surrounding 13×13 window.
fn surround_contrast(source: &[Color]) -> Vec<i16> {
    const RADIUS: usize = 6;
    let width = DETAIL_PLANE.width;
    let height = DETAIL_PLANE.height;
    let stride = width + 1;
    let mut sums = vec![0; stride * (height + 1)];
    for (y, row) in source.chunks_exact(width).enumerate() {
        let mut total = 0;
        for (x, &[luma, ..]) in row.iter().enumerate() {
            total += i32::from(luma);
            sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + total;
        }
    }
    let mut contrast = Vec::with_capacity(source.len());
    for (y, row) in source.chunks_exact(width).enumerate() {
        let top = y.saturating_sub(RADIUS);
        let bottom = (y + RADIUS + 1).min(height);
        for (x, &[luma, ..]) in row.iter().enumerate() {
            let left = x.saturating_sub(RADIUS);
            let right = (x + RADIUS + 1).min(width);
            let sum = sums[bottom * stride + right]
                - sums[bottom * stride + left]
                - sums[top * stride + right]
                + sums[top * stride + left];
            let count = (bottom - top) * (right - left);
            contrast.push(luma - (sum / count as i32) as i16);
        }
    }
    contrast
}

/// Bounds of the coarse foreground guide in fine-grid coordinates.
fn subject_bounds(subject: &[bool]) -> [usize; 4] {
    let (width, height) = (DETAIL_PLANE.width, DETAIL_PLANE.height);
    if subject.is_empty() {
        return [0, 0, width, height];
    }
    let mut bounds = [width, height, 0, 0];
    for (y, row) in subject.chunks_exact(W).enumerate() {
        if let Some(left) = row.iter().position(|&inside| inside) {
            let right = row.iter().rposition(|&inside| inside).unwrap_or(left) + 1;
            bounds = [
                bounds[0].min(2 * left),
                bounds[1].min(2 * y),
                bounds[2].max(2 * right),
                2 * (y + 1),
            ];
        }
    }
    bounds
}

/// Traces missing color marks whose nearby source samples have similar luminance.
fn chroma_outlines(
    source: &[Color],
    current: &[Color],
    subject: &[bool],
    [x0, y0, x1, y1]: [usize; 4],
) -> Vec<Outline> {
    let width = DETAIL_PLANE.width;
    let height = DETAIL_PLANE.height;
    let guided = !subject.is_empty();
    let (draft_tolerance, edge_tolerance) = if guided { (15, 8) } else { (10, 5) };
    let mut signal = Vec::new();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for y in y0.max(4)..y1.min(height - 4) {
        for x in x0.max(4)..x1.min(width - 4) {
            if guided && !subject[y / 2 * W + x / 2] {
                continue;
            }
            let i = y * width + x;
            let [light, u, v] = source[i];
            let [painted_light, painted_u, painted_v] = current[i];
            if light.abs_diff(painted_light) > draft_tolerance {
                continue;
            }
            let (du, dv) = (u - painted_u, v - painted_v);
            let (channel, missing) = if du.abs() >= dv.abs() { (1, du) } else { (2, dv) };
            if missing.abs() < 7 {
                continue;
            }
            let sign = missing.signum();
            let mut strength = 0;
            for offset in [2, 2 * width, 4, 4 * width] {
                let a = source[i - offset];
                let b = source[i + offset];
                if light.abs_diff(a[0]).max(light.abs_diff(b[0])) > edge_tolerance {
                    continue;
                }
                let peak = ((source[i][channel] - a[channel]) * sign)
                    .min((source[i][channel] - b[channel]) * sign);
                strength = strength.max(peak);
            }
            if strength >= 6 {
                if signal.is_empty() {
                    signal.resize(source.len(), 0);
                }
                signal[i] = strength.min(2 * missing.abs()) * sign;
                left = left.min(x);
                right = right.max(x + 1);
                top = top.min(y);
                bottom = bottom.max(y + 1);
            }
        }
    }
    if signal.is_empty() {
        Vec::new()
    } else {
        regions::compact_outlines_in(&signal, &[6, 12, 20], 24, [left, top, right, bottom])
    }
}

/// Traces shorter sections of source ridges that the broad fills did not capture.
fn ridge_fragments(
    source: &[Color],
    current: &[Color],
    input: &Input<'_>,
    [left, top, right, bottom]: [usize; 4],
) -> Vec<Outline> {
    const TILE: usize = 24;
    let width = DETAIL_PLANE.width;
    let height = DETAIL_PLANE.height;
    let mut signals = [Vec::new(), Vec::new()];
    let mut bounds = [width, height, 0, 0];
    let start_x = left.saturating_sub(2).max(4);
    let end_x = (right + 2).min(width - 4);
    let start_y = top.saturating_sub(2).max(4);
    let end_y = (bottom + 2).min(height - 4);
    for y in start_y..end_y {
        for x in start_x..end_x {
            let i = y * width + x;
            let ridge = input.signal[i];
            if ridge.abs() < 6 {
                continue;
            }
            let missing = (source[i][0] - current[i][0]) * ridge.signum();
            if missing < 9 {
                continue;
            }
            let k = coarse_index(i);
            if ![k, k - 1, k + 1, k - W, k + W].into_iter().any(|k| input.subject[k]) {
                continue;
            }
            let value = ridge.signum() * ridge.abs().min(2 * missing);
            for (stagger, signal) in [0, TILE / 2].into_iter().zip(&mut signals) {
                if (x + stagger).is_multiple_of(TILE) || (y + stagger).is_multiple_of(TILE) {
                    continue;
                }
                if signal.is_empty() {
                    signal.resize(source.len(), 0);
                }
                signal[i] = value;
            }
            let [left, top, right, bottom] = bounds;
            bounds = [left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1)];
        }
    }
    // Offset the second grid so a line cut at one tile boundary can still be traced there.
    signals
        .iter()
        .filter(|signal| !signal.is_empty())
        .flat_map(|signal| regions::compact_outlines_in(signal, &[8, 16], 16, bounds))
        .collect()
}

/// Rejects a fragment before building edge samples if it spills away from the missing line.
fn supports_fragment(
    outline: &Outline,
    signal: &[i16],
    source: &[Color],
    current: &[Color],
) -> bool {
    let geometry = Edges::new(outline, DETAIL_PLANE);
    let width = DETAIL_PLANE.width;
    let (mut left, mut right, mut area, mut hits) = (width, 0, 0, 0);
    for y in geometry.top..geometry.bottom {
        geometry.spans(y, |l, r| {
            left = left.min(l);
            right = right.max(r);
            area += r - l;
            hits += (y * width + l..y * width + r)
                .filter(|&i| {
                    let ridge = signal[i];
                    ridge.abs() >= 6 && ridge.signum() * (source[i][0] - current[i][0]) >= 6
                })
                .count();
        });
    }
    let length = right.saturating_sub(left).max(geometry.bottom - geometry.top);
    length >= 6 && area <= 6 * length && 3 * hits >= 2 * area
}

/// Appends fine polygons in painter's order while they improve the reconstructed features.
pub(super) fn append(
    scene: &mut Scene,
    input: Input<'_>,
    #[cfg(feature = "object-layers")] parts: Option<&mut super::parts::Parts>,
) {
    let source: Vec<_> = input.rgb.iter().copied().map(components).collect();
    let colors: Vec<_> = std::iter::once(scene.background)
        .chain(scene.patches.iter().map(|p| p.color))
        .map(|color| components(unpack(color)))
        .collect();
    let mut owners = scene.owners(DETAIL_PLANE);
    #[cfg(feature = "object-layers")]
    let marks = parts.map_or(&[][..], |parts| {
        parts.hints(input.rgb, &owners, scene, input.subject, input.signal)
    });
    let mut current: Vec<_> = owners.iter().map(|&i| colors[i as usize]).collect();
    let mut coverage = vec![false; source.len()];
    let mut proposals = regions::compact_outlines(input.signal, &[10, 20, 34], 192);
    if input.textured {
        // These candidates also capture one-sided contours, such as a narrow part against a busy backdrop.
        proposals.extend(regions::compact_outlines(&surround_contrast(&source), &[7, 14, 24], 90));
    } else {
        proposals.extend(regions::persistent_marks(&source, &current, input.signal));
    }
    #[cfg(feature = "object-layers")]
    if !marks.is_empty() {
        proposals.extend(regions::contextual_marks(&source, &current, input.signal, marks));
    }
    let bounds = subject_bounds(input.subject);
    let chroma = chroma_outlines(&source, &current, input.subject, bounds);
    let include_chroma = !chroma.is_empty();
    let fragments = if input.textured && !input.subject.is_empty() {
        ridge_fragments(&source, &current, &input, bounds)
    } else {
        Vec::new()
    };
    // Older marks are just proposals. Their contrast must still improve the current painting.
    let translated = input.previous.iter().take(16).filter_map(|patch| {
        motion::follow(&patch.outline, input.previous_rgb, &source, &owners)
            .map(|outline| (outline, Some(patch.color)))
    });
    let mut seen = HashSet::new();
    let mut candidates: Vec<_> = input
        .previous
        .iter()
        .map(|p| (p.outline, Some(p.color), false))
        .chain(translated.map(|(outline, color)| (outline, color, false)))
        .chain(proposals.into_iter().map(|outline| (outline, None, false)))
        .chain(fragments.into_iter().map(|outline| (outline, None, true)))
        .chain(chroma.into_iter().map(|outline| (outline, None, false)))
        .filter(|&(outline, _, _)| seen.insert(outline))
        .filter(|&(outline, _, fragment)| {
            !fragment || supports_fragment(&outline, input.signal, &source, &current)
        })
        .filter_map(|(outline, color, _)| {
            Candidate::new(
                outline,
                &input,
                &source,
                &mut coverage,
                color,
                #[cfg(feature = "object-layers")]
                marks,
            )
        })
        .map(|candidate| {
            let gain = candidate.gain(&source, &current);
            (candidate, gain)
        })
        .collect();

    const MIN_GAIN: i64 = 650;
    let subject_slots =
        if input.textured && !input.subject.is_empty() { input.slots / 2 } else { 0 };
    let mut painted_subject = 0usize;
    #[cfg(feature = "object-layers")]
    let mut painted_marks = [0i64; super::parts::MAX_MARKS + 1];
    for remaining in (1..=input.slots).rev() {
        let best = candidates
            .iter()
            .map(|&(_, gain)| gain)
            .enumerate()
            .max_by_key(|&(_, gain)| gain)
            .filter(|&(_, gain)| gain > MIN_GAIN);
        let Some((best, gain)) = best else { break };
        // Give the subject room for detail when stronger background edges dominate the ranking.
        let selected = if remaining <= subject_slots.saturating_sub(painted_subject) {
            let minimum = MIN_GAIN.max(gain / 5);
            candidates
                .iter()
                .enumerate()
                .filter(|(_, (candidate, gain))| candidate.on_subject && *gain > minimum)
                .max_by_key(|(_, (_, gain))| *gain)
                .map_or(best, |(i, _)| i)
        } else {
            best
        };
        #[cfg(feature = "object-layers")]
        let selected = if !marks.is_empty() && remaining >= input.slots.div_ceil(2) {
            candidates
                .iter()
                .enumerate()
                .filter(|(_, (candidate, improvement))| {
                    let floor = if candidate.on_subject || input.subject.is_empty() {
                        gain / 20
                    } else {
                        gain / 6
                    };
                    candidate.mark != 0 && *improvement > MIN_GAIN.max(floor)
                })
                .map(|(i, (candidate, _))| {
                    let local = candidate.guided_gain(&source, &current, marks);
                    (i, local / (1 + painted_marks[candidate.mark]))
                })
                .filter(|&(_, local)| local > 400)
                .max_by_key(|&(_, local)| local)
                .map_or(selected, |(i, _)| i)
        } else {
            selected
        };
        // Give missing edges room after the highest-gain layers have established the fine fills.
        let selected =
            if remaining <= 4 && input.slots >= 4 && input.textured && !input.subject.is_empty() {
                let minimum = MIN_GAIN.max(gain / 24);
                let ordinary = candidates[selected].0.restored_contrast(&current, include_chroma);
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, (candidate, gain))| {
                        candidate.on_subject && candidate.pixels.len() <= 250 && *gain > minimum
                    })
                    .map(|(i, (candidate, _))| {
                        (i, candidate.restored_contrast(&current, include_chroma))
                    })
                    .filter(|&(_, contrast)| contrast > 125 && contrast > ordinary + 40)
                    .max_by_key(|&(_, contrast)| contrast)
                    .map_or(selected, |(i, _)| i)
            } else {
                selected
            };
        let (mut choice, _) = candidates.swap_remove(selected);
        #[cfg(feature = "object-layers")]
        let adjust_tone = marks.is_empty();
        #[cfg(not(feature = "object-layers"))]
        let adjust_tone = true;
        if adjust_tone {
            choice.refit_tone(&source, &current);
        }
        #[cfg(feature = "object-layers")]
        {
            painted_marks[choice.mark] += 1;
        }
        painted_subject += usize::from(choice.on_subject);
        let owner = (scene.patches.len() + 1) as u16;
        for i in choice.pixels {
            current[i] = choice.fill;
            owners[i] = owner;
        }
        for (candidate, gain) in &mut candidates {
            if candidate.affected(choice.bounds) {
                *gain = candidate.gain(&source, &current);
            }
        }
        scene.patches.push(choice.patch);
    }
    contrast::refit(scene, &source, &owners);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the detailed fitter with image-derived input.
    fn append(scene: &mut Scene, input: Input<'_>) {
        super::append(
            scene,
            input,
            #[cfg(feature = "object-layers")]
            None,
        );
    }

    /// Restores the contrast of a dark mark when the underlying flat fill is too dark.
    #[test]
    fn adjusts_a_mark_to_the_underlying_tone() {
        let outline =
            [[76, 94], [94, 94], [94, 98], [76, 98], [76, 98], [76, 98], [76, 98], [76, 98]];
        let count = DETAIL_PLANE.width * DETAIL_PLANE.height;
        let mut rgb = vec![[178; 3]; count];
        let edges = Edges::new(&outline, DETAIL_PLANE);
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| {
                let start = y * DETAIL_PLANE.width;
                rgb[start + l..start + r].fill([134; 3]);
            });
        }
        let input = Input {
            rgb: &rgb,
            signal: &[],
            subject: &[],
            previous: &[],
            previous_rgb: &[],
            slots: 1,
            textured: false,
        };
        let source: Vec<_> = rgb.iter().copied().map(components).collect();
        let draft = vec![components(unpack(pack([150; 3]))); count];
        let mut candidate = Candidate::new(
            outline,
            &input,
            &source,
            &mut vec![false; count],
            None,
            #[cfg(feature = "object-layers")]
            &[],
        )
        .unwrap();
        let before = candidate.fill[0];
        let gain = candidate.gain(&source, &draft);
        candidate.refit_tone(&source, &draft);
        assert!(candidate.fill[0] <= before - 4);
        assert!(candidate.gain(&source, &draft) > gain);
        assert!(candidate.restored_contrast(&draft, false) > 0);
        assert_eq!(candidate.restored_contrast(&source, false), 0);
    }

    /// A proposed fill scores lower when it erases a visible edge inside its outline.
    #[test]
    fn preserves_visible_edges_inside_a_proposal() {
        let candidate = Candidate {
            patch: Patch::polygon([[0, 0]; 8], pack([120; 3])),
            fill: [120, 0, 0],
            pixels: Vec::new(),
            edges: (0..9)
                .flat_map(|i| {
                    [false, true].map(|interior| Edge {
                        first: i * 4 + usize::from(interior),
                        second: i * 4 + 3 - usize::from(interior),
                        reference: [-30, 0, 0],
                        interior,
                        #[cfg(feature = "object-layers")]
                        boundary: false,
                    })
                })
                .collect(),
            weight: 8,
            bounds: [0; 4],
            on_subject: true,
            #[cfg(feature = "object-layers")]
            mark: 0,
        };
        let mut draft = [[150, 0, 0]; 36];
        let recovered = candidate.restored_contrast(&draft, false);
        for pixels in draft.chunks_exact_mut(4) {
            pixels[1][0] = 120;
        }
        let covered = candidate.restored_contrast(&draft, false);
        assert!(recovered > covered && covered > 0);
    }

    /// Traces a one-pixel stroke and reproduces its dark edge on the fine grid.
    #[test]
    fn fits_thin_marks_after_the_base_surface() {
        let width = DETAIL_PLANE.width;
        let mut rgb = vec![[170; 3]; width * DETAIL_PLANE.height];
        let mut signal = vec![0; rgb.len()];
        for x in 117..149 {
            let i = 93 * width + x;
            rgb[i] = [55; 3];
            signal[i] = -115;
        }
        let mut scene = Scene { background: pack([170; 3]), patches: Vec::new() };
        append(
            &mut scene,
            Input {
                rgb: &rgb,
                signal: &signal,
                subject: &[],
                previous: &[],
                previous_rgb: &[],
                slots: 1,
                textured: false,
            },
        );
        assert_eq!(scene.len(), 1);
        let output = scene.render(DETAIL_PLANE);
        assert!(output[3 * (93 * width + 130)] < 75);
        assert!(output[3 * (91 * width + 130)] > 150);
    }

    /// A thin closed line yields supported fragments that leave its center clear.
    #[test]
    fn traces_fragments_of_a_thin_loop() {
        let width = DETAIL_PLANE.width;
        let count = width * DETAIL_PLANE.height;
        let mut rgb = vec![[160; 3]; count];
        let mut signal = vec![0; count];
        for y in 80..111 {
            for x in 95..138 {
                if !(82..109).contains(&y) || !(97..136).contains(&x) {
                    rgb[y * width + x] = [120; 3];
                    signal[y * width + x] = -40;
                }
            }
        }
        let source: Vec<_> = rgb.iter().copied().map(components).collect();
        let draft = vec![components([160; 3]); count];
        let subject = vec![true; W * DETAIL_PLANE.height / 2];
        let input = Input {
            rgb: &rgb,
            signal: &signal,
            subject: &subject,
            previous: &[],
            previous_rgb: &[],
            slots: 4,
            textured: true,
        };
        let mut coverage = vec![false; count];
        let mut top_and_bottom = [false; 2];
        for outline in ridge_fragments(&source, &draft, &input, subject_bounds(&subject)) {
            if !supports_fragment(&outline, &signal, &source, &draft) {
                continue;
            }
            let candidate = Candidate::new(
                outline,
                &input,
                &source,
                &mut coverage,
                None,
                #[cfg(feature = "object-layers")]
                &[],
            )
            .unwrap();
            assert!(!candidate.pixels.contains(&(94 * width + 117)));
            for &i in &candidate.pixels {
                top_and_bottom[0] |= i / width == 80;
                top_and_bottom[1] |= i / width == 110;
            }
        }
        assert!(top_and_bottom.into_iter().all(|side| side));
    }

    /// Retains a thin color mark when its luminance matches the underlying surface.
    #[test]
    fn fits_a_color_mark_without_a_luminance_edge() {
        let width = DETAIL_PLANE.width;
        let color = pack([145; 3]);
        let base = unpack(color);
        let mark = [base[0] + 28, base[1] - 12, base[2] - 4];
        assert_eq!(luma(mark), luma(base));
        let mut rgb = vec![base; width * DETAIL_PLANE.height];
        for y in 93..96 {
            rgb[y * width + 117..y * width + 135].fill(mark);
        }
        let mut scene = Scene { background: color, patches: Vec::new() };
        append(
            &mut scene,
            Input {
                rgb: &rgb,
                signal: &vec![0; rgb.len()],
                subject: &[],
                previous: &[],
                previous_rgb: &[],
                slots: 1,
                textured: false,
            },
        );
        assert_eq!(scene.len(), 1);
        let output = scene.render(DETAIL_PLANE);
        let pixel = |y, x| &output[3 * (y * width + x)..][..3];
        let inside = pixel(94, 125);
        assert!(i16::from(inside[0]) - i16::from(inside[1]) >= 20);
        assert_eq!(pixel(90, 125), base);
    }

    /// Fits a color-defined line in the guide despite a small source and draft brightness difference.
    #[test]
    fn fits_a_color_mark_with_a_small_luminance_edge() {
        let width = DETAIL_PLANE.width;
        let color = pack([140; 3]);
        let painted = unpack(color);
        let base = painted.map(|v| v + 5);
        let mark = [base[0] + 40, base[1] - 6, base[2]];
        assert_eq!(luma(mark) - luma(base), 7);
        assert_eq!(luma(mark) - luma(painted), 12);
        let mut rgb = vec![base; width * DETAIL_PLANE.height];
        let mut subject = vec![false; W * DETAIL_PLANE.height / 2];
        for row in subject.chunks_exact_mut(W).take(55).skip(42) {
            row[54..72].fill(true);
        }
        for y in 93..96 {
            rgb[y * width + 117..y * width + 135].fill(mark);
            rgb[y * width + 201..y * width + 219].fill(mark);
        }
        let mut scene = Scene { background: color, patches: Vec::new() };
        append(
            &mut scene,
            Input {
                rgb: &rgb,
                signal: &vec![0; rgb.len()],
                subject: &subject,
                previous: &[],
                previous_rgb: &[],
                slots: 1,
                textured: false,
            },
        );
        assert_eq!(scene.len(), 1);
        let output = scene.render(DETAIL_PLANE);
        let pixel = |y, x| &output[3 * (y * width + x)..][..3];
        let inside = pixel(94, 125);
        assert!(i16::from(inside[0]) - i16::from(inside[1]) >= 30);
        assert_eq!(pixel(94, 209), painted);
    }

    /// Recovers a narrow protrusion from surrounding contrast when ridges supply no candidates.
    #[test]
    fn fits_a_protrusion_along_a_surface_boundary() {
        let width = DETAIL_PLANE.width;
        let mut rgb = vec![[170; 3]; width * DETAIL_PLANE.height];
        for y in 70..110 {
            rgb[y * width + 72..y * width + 152].fill([160; 3]);
        }
        for y in 110..128 {
            rgb[y * width + 116..y * width + 121].fill([70; 3]);
        }
        let mut scene = Scene {
            background: pack([170; 3]),
            patches: vec![Patch {
                outline: [
                    [48, 83],
                    [101, 83],
                    [101, 130],
                    [48, 130],
                    [48, 130],
                    [48, 130],
                    [48, 130],
                    [48, 130],
                ],
                color: pack([160; 3]),
                stroke: None,
            }],
        };
        append(
            &mut scene,
            Input {
                rgb: &rgb,
                signal: &vec![0; rgb.len()],
                subject: &[],
                previous: &[],
                previous_rgb: &[],
                slots: 1,
                textured: true,
            },
        );
        assert_eq!(scene.len(), 2);
        let output = scene.render(DETAIL_PLANE);
        assert!(output[3 * (118 * width + 118)] < 110);
        assert!(output[3 * (118 * width + 126)] > 150);
    }

    /// Finds a compact mark below the fixed ridge thresholds without painting its surround.
    #[test]
    fn fits_a_low_contrast_mark_on_a_flat_surface() {
        let width = DETAIL_PLANE.width;
        let color = pack([150; 3]);
        let mut rgb = vec![unpack(color); width * DETAIL_PLANE.height];
        let mut signal = vec![0; rgb.len()];
        for y in 101..104 {
            let start = y * width + 139;
            rgb[start..start + 21].fill([137; 3]);
            signal[start..start + 21].fill(-8);
        }
        let mut scene = Scene { background: color, patches: Vec::new() };
        append(
            &mut scene,
            Input {
                rgb: &rgb,
                signal: &signal,
                subject: &[],
                previous: &[],
                previous_rgb: &[],
                slots: 1,
                textured: false,
            },
        );
        assert_eq!(scene.len(), 1);
        let output = scene.render(DETAIL_PLANE);
        assert!(output[3 * (102 * width + 150)] < 143);
        assert_eq!(output[3 * (98 * width + 150)], unpack(color)[0]);
    }

    /// Keeps useful foreground marks when stronger background marks compete for the fine slots.
    #[test]
    fn fits_subject_detail_in_a_textured_scene() {
        let width = DETAIL_PLANE.width;
        let background = pack([150; 3]);
        let mut rgb = vec![unpack(background); width * DETAIL_PLANE.height];
        let mut signal = vec![0; rgb.len()];
        for (x, y, tone) in
            [(30, 38, 99), (95, 38, 102), (220, 38, 104), (295, 38, 106), (140, 100, 123)]
        {
            for row in y..y + 4 {
                let start = row * width + x;
                rgb[start..start + 20].fill([tone; 3]);
                signal[start..start + 20].fill(i16::from(tone) - 150);
            }
        }
        let mut subject = vec![false; W * (DETAIL_PLANE.height / 2)];
        for y in 44..59 {
            subject[y * W + 62..y * W + 88].fill(true);
        }
        let render = |subject: &[bool]| {
            let mut scene = Scene { background, patches: Vec::new() };
            append(
                &mut scene,
                Input {
                    rgb: &rgb,
                    signal: &signal,
                    subject,
                    previous: &[],
                    previous_rgb: &[],
                    slots: 4,
                    textured: true,
                },
            );
            assert_eq!(scene.len(), 4);
            scene.render(DETAIL_PLANE)
        };
        let mark = 3 * (101 * width + 149);
        assert_eq!(render(&[])[mark], unpack(background)[0]);
        assert!(render(&subject)[mark] < 130);
    }
}
