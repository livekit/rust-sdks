//! Checks for camera motion before discarding object contours after a large frame change.
use super::{H, W};

const RADIUS: isize = 16;
const MARGIN: usize = 24;
const SAMPLES: usize = 25;

/// Finds texture patches that agree on a translation across the frame.
pub(super) fn coherent(old: &[[u8; 3]], new: &[[u8; 3]]) -> bool {
    let mut matches = Vec::with_capacity(20);
    let mut eligible = 0;
    for (row, y) in (MARGIN..H - MARGIN).step_by(19).enumerate() {
        for (column, x) in (MARGIN..W - MARGIN).step_by(29).enumerate() {
            let indices: [_; SAMPLES] =
                std::array::from_fn(|i| (y + 3 * (i / 5) - 6) * W + x + 3 * (i % 5) - 6);
            let patch = indices.map(|i| old[i]);
            let texture: u32 = patch.iter().map(|&sample| difference(sample, patch[12])).sum();
            if texture < 12 * 3 * SAMPLES as u32 {
                continue;
            }
            eligible += 1;
            if let Some((dx, dy)) = displacement(new, &indices, &patch) {
                matches.push((dx, dy, row, column));
            }
        }
    }
    matches.iter().any(|&(dx, dy, _, _)| {
        let (mut votes, mut rows, mut columns) = (0, 0u32, 0u32);
        for &(x, y, row, column) in &matches {
            if dx.abs_diff(x) <= 3 && dy.abs_diff(y) <= 3 {
                votes += 1;
                rows |= 1 << row;
                columns |= 1 << column;
            }
        }
        votes >= 5 && votes * 2 >= eligible && rows.count_ones() >= 2 && columns.count_ones() >= 3
    })
}

/// Matches a patch on a two-pixel grid and refines the best shift near its center.
fn displacement(
    new: &[[u8; 3]],
    indices: &[usize; SAMPLES],
    patch: &[[u8; 3]; SAMPLES],
) -> Option<(isize, isize)> {
    let score = |dx: isize, dy: isize, limit: u32| {
        let offset = dy * W as isize + dx;
        let mut loss = 0;
        for (&i, &sample) in indices.iter().zip(patch) {
            // Patch centers and search bounds keep every index within the image.
            loss += difference(sample, new[i.wrapping_add_signed(offset)]);
            if loss >= limit {
                break;
            }
        }
        loss
    };
    let limit = (score(0, 0, u32::MAX) * 2 / 3).min(18 * 3 * SAMPLES as u32);
    let mut best = (limit, 0, 0);
    for dy in (-RADIUS..=RADIUS).step_by(2) {
        for dx in (-RADIUS..=RADIUS).step_by(2) {
            let loss = score(dx, dy, best.0);
            if loss < best.0 {
                best = (loss, dx, dy);
            }
        }
    }
    if best.0 == limit {
        return None;
    }
    let (_, dx, dy) = best;
    for y in (dy - 1).max(-RADIUS)..=(dy + 1).min(RADIUS) {
        for x in (dx - 1).max(-RADIUS)..=(dx + 1).min(RADIUS) {
            let loss = score(x, y, best.0);
            if loss < best.0 {
                best = (loss, x, y);
            }
        }
    }
    Some((best.1, best.2))
}

/// Returns the sum of channel distances for one pixel pair.
fn difference(a: [u8; 3], b: [u8; 3]) -> u32 {
    u32::from(a[0].abs_diff(b[0])) + u32::from(a[1].abs_diff(b[1])) + u32::from(a[2].abs_diff(b[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Distinguishes a frame-wide pan from a cut and from motion in one narrow column.
    #[test]
    fn requires_distributed_motion() {
        let mut seed = 13u32;
        let mut scene = || {
            (0..W * H)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let [_, r, g, b] = seed.to_le_bytes();
                    [r, g, b]
                })
                .collect::<Vec<_>>()
        };
        let old = scene();
        let unrelated = scene();
        let pan: Vec<_> =
            (0..old.len()).map(|i| old[((i / W + H - 4) % H) * W + (i % W + W - 12) % W]).collect();
        let local: Vec<_> = old
            .iter()
            .zip(&pan)
            .enumerate()
            .map(|(i, (&old, &pan))| if (W / 3..W / 2).contains(&(i % W)) { pan } else { old })
            .collect();
        assert!(coherent(&old, &pan));
        assert!(!coherent(&old, &unrelated));
        assert!(!coherent(&old, &local));
    }
}
