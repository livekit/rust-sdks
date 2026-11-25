//! Follows compact contours with source edges when the current draft still lacks that contrast.
use super::{components, difference, error, Color, Edges, Outline, DETAIL_PLANE};
use crate::video::Canvas;

const RADIUS: usize = 6;

/// Source colors just inside and outside one edge of the previous outline.
struct Sample {
    inside: usize,
    outside: usize,
    inner: Color,
    outer: Color,
    contrast: Color,
    strength: i16,
}

/// A displacement in fine-plane pixels and its contrast-matching cost.
#[derive(Clone, Copy)]
struct Match {
    offset: [isize; 2],
    cost: i64,
}

/// Samples the previous contour on both axes, leaving room for the search window.
fn sample_edges(outline: &Outline, previous: &[[u8; 3]]) -> Vec<Sample> {
    let Canvas { width, height } = DETAIL_PLANE;
    let transposed = outline.map(|[x, y]| [y, x]);
    let mut samples = Vec::new();
    for (points, plane, vertical) in [
        (outline, DETAIL_PLANE, false),
        (&transposed, Canvas { width: height, height: width }, true),
    ] {
        let edges = Edges::new(points, plane);
        for row in (edges.top..edges.bottom).step_by(2) {
            if !(RADIUS..plane.height - RADIUS).contains(&row) {
                continue;
            }
            edges.spans(row, |left, right| {
                let index =
                    |column| if vertical { column * width + row } else { row * width + column };
                let columns = RADIUS..plane.width - RADIUS;
                for (a, b) in [(left, left.saturating_sub(2)), (right - 1, right + 1)] {
                    if !columns.contains(&a) || !columns.contains(&b) {
                        continue;
                    }
                    let (inside, outside) = (index(a), index(b));
                    let (inner, outer) =
                        (components(previous[inside]), components(previous[outside]));
                    let contrast = difference(inner, outer);
                    let strength = contrast[0].abs() + contrast[1].abs().max(contrast[2].abs()) / 2;
                    if strength >= 9 {
                        samples.push(Sample { inside, outside, inner, outer, contrast, strength });
                    }
                }
            });
        }
    }
    samples.sort_by_key(|sample| std::cmp::Reverse(sample.strength));
    samples.truncate(16);
    samples
}

/// Compares the same edge neighborhoods after shifting, giving contrast more weight than color.
fn match_cost(samples: &[Sample], current: &[Color], [x, y]: [isize; 2]) -> i64 {
    let displacement = x + y * DETAIL_PLANE.width as isize;
    let total: i64 = samples
        .iter()
        .map(|sample| {
            let inner = current[sample.inside.wrapping_add_signed(displacement)];
            let outer = current[sample.outside.wrapping_add_signed(displacement)];
            let contrast = error(sample.contrast, difference(inner, outer));
            let colors = error(sample.inner, inner) + error(sample.outer, outer);
            i64::from(2 * contrast + colors / 4)
        })
        .sum();
    total / samples.len() as i64
}

/// Searches a coarse grid and then checks the adjacent pixels around its best two matches.
fn search(score: impl Fn([isize; 2]) -> i64) -> Match {
    let radius = RADIUS as isize;
    let zero = Match { offset: [0, 0], cost: score([0, 0]) };
    let at = |x: isize, y: isize| Match {
        offset: [x, y],
        cost: score([x, y]) + 16 * (x.unsigned_abs() + y.unsigned_abs()) as i64,
    };
    let mut best = [zero; 2];
    for y in (-radius..=radius).step_by(2) {
        for x in (-radius..=radius).step_by(2) {
            let candidate = at(x, y);
            if candidate.cost < best[0].cost {
                best = [candidate, best[0]];
            } else if candidate.cost < best[1].cost && candidate.offset != best[0].offset {
                best[1] = candidate;
            }
        }
    }
    let mut refined = best[0];
    for Match { offset: [x, y], .. } in best {
        for row in (y - 1).max(-radius)..=(y + 1).min(radius) {
            for column in (x - 1).max(-radius)..=(x + 1).min(radius) {
                if [column, row] == [x, y] {
                    continue;
                }
                let candidate = at(column, row);
                if candidate.cost < refined.cost {
                    refined = candidate;
                }
            }
        }
    }
    refined
}

/// Proposes a moved outline when its source edges match and mostly lie inside one draft fill.
pub(super) fn follow(
    outline: &Outline,
    previous: &[[u8; 3]],
    current: &[Color],
    owners: &[u16],
) -> Option<Outline> {
    if previous.len() != current.len() {
        return None;
    }
    let samples = sample_edges(outline, previous);
    if samples.len() < 4 {
        return None;
    }
    let score = |offset| match_cost(&samples, current, offset);
    let zero = score([0, 0]);
    let best = search(score);
    let [x, y] = best.offset;
    if best.offset == [0, 0] || best.cost * 5 > zero * 4 || zero - best.cost < 170 {
        return None;
    }
    let margin = score(best.offset) + 110;
    for [dx, dy] in [[-2, 0], [2, 0], [0, -2], [0, 2]] {
        let other = [x + dx, y + dy];
        if other.iter().all(|v| v.unsigned_abs() <= RADIUS) && score(other) < margin {
            return None;
        }
    }
    let displacement = x + y * DETAIL_PLANE.width as isize;
    let missing = samples
        .iter()
        .filter(|sample| {
            owners[sample.inside.wrapping_add_signed(displacement)]
                == owners[sample.outside.wrapping_add_signed(displacement)]
        })
        .count();
    if missing * 10 < samples.len() * 7 {
        return None;
    }
    let quantize =
        |d: isize, n: usize| ((255 * d + d.signum() * n as isize / 2) / n as isize) as i16;
    let shift = [quantize(x, DETAIL_PLANE.width), quantize(y, DETAIL_PLANE.height)];
    let mut translated = *outline;
    for point in &mut translated {
        for (coordinate, delta) in point.iter_mut().zip(shift) {
            *coordinate = u8::try_from(i16::from(*coordinate) + delta).ok()?;
        }
    }
    Some(translated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Follows a marked surface while deferring to edges that the current draft already represents.
    #[test]
    fn follows_motion_inside_a_flat_fill() {
        let outline =
            [[55, 50], [67, 50], [67, 59], [55, 59], [55, 59], [55, 59], [55, 59], [55, 59]];
        let shifted = outline.map(|[x, y]| [x + 2, y]);
        let paint = |points| {
            let edges = Edges::new(points, DETAIL_PLANE);
            let width = DETAIL_PLANE.width;
            let mut colors = vec![[178; 3]; width * DETAIL_PLANE.height];
            for row in edges.top..edges.bottom {
                edges.spans(row, |left, right| {
                    colors[row * width + left..row * width + right].fill([84; 3])
                });
            }
            colors
        };
        let previous = paint(&outline);
        let rgb = paint(&shifted);
        let current: Vec<_> = rgb.iter().copied().map(components).collect();
        let flat = vec![0; rgb.len()];
        assert_eq!(follow(&outline, &previous, &current, &flat), Some(shifted));
        let drawn: Vec<_> = rgb.iter().map(|color| u16::from(color[0] == 84)).collect();
        assert_eq!(follow(&outline, &previous, &current, &drawn), None);
    }
}
