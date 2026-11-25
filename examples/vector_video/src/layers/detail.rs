//! Local lines and compact features that persist when broad lighting varies.
use super::{fit::luma, Canvas, DETAIL_PLANE, H, PLANE, W};

/// Signed local contrast and its absolute magnitude summed along each row.
pub(super) struct Detail {
    pub(super) contrast: Vec<i16>,
    pub(super) fine: Vec<i16>,
    pub(super) samples: Vec<Option<[u8; 3]>>,
    pub(super) dense_signal: Vec<i16>,
    pub(super) edges: Vec<u8>,
    pub(super) strong_edges: usize,
    prefix: Vec<i32>,
}

impl Detail {
    /// Measures compact detail with a single inexpensive stencil on the fitting grid.
    pub(super) fn budgeted(rgb: &[[u8; 3]], dense: &[[u8; 3]]) -> Self {
        let mut fine = contrast_ridges(rgb, PLANE, 2, &[2, 2 * W, 2 * (W + 1), 2 * (W - 1)]);
        let samples =
            if dense.is_empty() { Vec::new() } else { budgeted_samples(dense, &mut fine) };
        let mut prefix = Vec::with_capacity(H * (W + 1));
        for row in fine.chunks_exact(W) {
            let mut sum = 0;
            prefix.push(0);
            prefix.extend(row.iter().map(|value| {
                sum += i32::from(value.abs());
                sum
            }));
        }
        Self {
            contrast: Vec::new(),
            fine,
            samples,
            dense_signal: Vec::new(),
            edges: Vec::new(),
            strong_edges: 0,
            prefix,
        }
    }

    /// Finds bright and dark features against both sides of their local surround.
    pub(super) fn new(luma: &[i32], rgb: &[[u8; 3]], dense: &[[u8; 3]]) -> Self {
        let levels: Vec<_> = luma
            .iter()
            .zip(rgb)
            .map(|(&y, &[r, g, b])| {
                let [r, g, b] = [r, g, b].map(i32::from);
                [y, (r - b) / 2, (2 * g - r - b) / 4]
            })
            .collect();
        let at = |x: usize, y: usize| levels[y * W + x];
        let mut smooth = vec![[0; 3]; W * H];
        for y in 0..H {
            for x in 0..W {
                let i = y * W + x;
                let taps = [
                    at(x.saturating_sub(1), y),
                    at((x + 1).min(W - 1), y),
                    at(x, y.saturating_sub(1)),
                    at(x, (y + 1).min(H - 1)),
                ];
                smooth[i] = std::array::from_fn(|c| {
                    (4 * levels[i][c] + taps.iter().map(|tap| tap[c]).sum::<i32>() + 4) / 8
                });
            }
        }
        let mut contrast = vec![0; W * H];
        let mut fine = vec![0; W * H];
        ridge_responses(&smooth, &mut fine, &mut contrast);
        let dense_signal = if dense.is_empty() { Vec::new() } else { dense_contrast(dense) };
        let samples =
            if dense.is_empty() { Vec::new() } else { fine_samples(dense, &dense_signal) };
        let mut chosen = Vec::with_capacity(samples.len());
        let mut edges = vec![0; W * H];
        let mut strong_edges = 0;
        let mut prefix = vec![0; (W + 1) * H];
        for y in 0..H {
            let mut row = 0;
            for x in 0..W {
                let i = y * W + x;
                let (mut small, mut strength) = (i32::from(fine[i]), i32::from(contrast[i]));
                if let Some(&(signal, color)) = samples.get(i) {
                    let restore = signal.abs() >= small.abs().max(8) + 4;
                    if restore {
                        small = signal;
                    }
                    if signal.abs() > strength.abs() {
                        strength = signal;
                    }
                    chosen.push(restore.then_some(color));
                }
                fine[i] = small as i16;
                contrast[i] = strength as i16;
                let dx = at(x.saturating_sub(1), y);
                let rx = at((x + 1).min(W - 1), y);
                let dy = at(x, y.saturating_sub(1));
                let ry = at(x, (y + 1).min(H - 1));
                edges[i] = (0..3)
                    .map(|c| (dx[c] - rx[c]).abs().max((dy[c] - ry[c]).abs()))
                    .max()
                    .unwrap_or(0) as u8;
                strong_edges += usize::from(edges[i] >= 24);
                row += strength.abs();
                prefix[y * (W + 1) + x + 1] = row;
            }
        }
        Self { contrast, fine, samples: chosen, dense_signal, edges, strong_edges, prefix }
    }

    /// Checks whether strong features already have support at the coarse analysis resolution.
    pub(super) fn supports_coarse_marks(&self, subject: &[bool]) -> bool {
        let (mut area, mut supported) = (0, 0);
        for (i, (&response, sample)) in self.fine.iter().zip(&self.samples).enumerate() {
            if subject.get(i) == Some(&false) {
                continue;
            }
            area += 1;
            supported += usize::from(response.abs() >= 10 && sample.is_none());
        }
        area >= W * H / 12 && supported * 10 >= area
    }

    /// Sums detail signal under one span of a proposed outline.
    pub(super) fn span(&self, y: usize, l: usize, r: usize) -> i32 {
        let row = y * (W + 1);
        self.prefix[row + r] - self.prefix[row + l]
    }
}

/// Checks a small neighborhood around the strongest unresolved edge in each tile.
fn budgeted_samples(rgb: &[[u8; 3]], fine: &mut [i16]) -> Vec<Option<[u8; 3]>> {
    const TILE: usize = 8;
    const WIDTH: usize = DETAIL_PLANE.width;
    let light: Vec<_> = rgb.iter().copied().map(|pixel| luma(pixel) as i16).collect();
    let pixels = |x: usize, y: usize| {
        let i = 2 * (y * WIDTH + x);
        [i, i + 1, i + WIDTH, i + WIDTH + 1]
    };
    let response = |i: usize| {
        let center = light[i];
        [3, 3 * WIDTH].into_iter().fold(0i16, |best, offset| {
            let a = center - light[i - offset];
            let b = center - light[i + offset];
            let value = a.min(b).max(0) + a.max(b).min(0);
            if value.abs() > best.abs() {
                value
            } else {
                best
            }
        })
    };
    let mut samples = vec![None; W * H];
    for top in (2..H - 2).step_by(TILE) {
        let bottom = (top + TILE).min(H - 2);
        for left in (2..W - 2).step_by(TILE) {
            let right = (left + TILE).min(W - 2);
            let (mut best, mut chosen) = (12, (left, top));
            for y in top..bottom {
                for x in left..right {
                    let [a, b, c, d] = pixels(x, y).map(|i| light[i]);
                    let range = a.max(b).max(c).max(d) - a.min(b).min(c).min(d);
                    let score = range.min(56) - fine[y * W + x].abs() / 3;
                    if score > best {
                        best = score;
                        chosen = (x, y);
                    }
                }
            }
            if best == 12 {
                continue;
            }
            let (x, y) = chosen;
            for y in (y - 1).max(top)..(y + 2).min(bottom) {
                for x in (x - 1).max(left)..(x + 2).min(right) {
                    let i = y * W + x;
                    let points = pixels(x, y);
                    let values = points.map(response);
                    for (a, b) in [(0, 1), (2, 3), (0, 2), (1, 3)] {
                        let value =
                            values[a].min(values[b]).max(0) + values[a].max(values[b]).min(0);
                        if value.abs() >= fine[i].abs().max(8) + 5 {
                            fine[i] = value;
                            let (first, second) = (rgb[points[a]], rgb[points[b]]);
                            samples[i] = Some(std::array::from_fn(|c| {
                                u16::midpoint(first[c].into(), second[c].into()) as u8
                            }));
                        }
                    }
                }
            }
        }
    }
    samples
}

/// Measures dark and light ridges at every pixel of the fine analysis grid.
fn dense_contrast(rgb: &[[u8; 3]]) -> Vec<i16> {
    let width = DETAIL_PLANE.width;
    let offsets = [
        3,
        3 * width,
        3 * (width + 1),
        3 * (width - 1),
        4,
        4 * width,
        4 * (width + 1),
        4 * (width - 1),
    ];
    contrast_ridges(rgb, DETAIL_PLANE, 4, &offsets)
}

/// Applies the given directional stencils on the interior of a raster.
fn contrast_ridges(rgb: &[[u8; 3]], grid: Canvas, border: usize, offsets: &[usize]) -> Vec<i16> {
    let Canvas { width, height } = grid;
    let levels: Vec<_> = rgb.iter().copied().map(|pixel| luma(pixel) as i16).collect();
    let mut signal = vec![0i16; levels.len()];
    for y in border..height - border {
        let start = y * width + border;
        let end = (y + 1) * width - border;
        let row = &mut signal[start..end];
        let centers = &levels[start..end];
        // Keep each stencil as contiguous slices so the compiler can evaluate adjacent pixels together.
        for &offset in offsets {
            let before = &levels[start - offset..end - offset];
            let after = &levels[start + offset..end + offset];
            for (((out, &center), &before), &after) in
                row.iter_mut().zip(centers).zip(before).zip(after)
            {
                let a = center - before;
                let b = center - after;
                let peak = a.min(b).max(0) + a.max(b).min(0);
                *out = if peak.abs() > out.abs() { peak } else { *out };
            }
        }
    }
    signal
}

/// Finds subcell marks supported by two nearby samples and preserves their measured colors.
fn fine_samples(rgb: &[[u8; 3]], signal: &[i16]) -> Vec<(i32, [u8; 3])> {
    let width = DETAIL_PLANE.width;
    (0..W * H)
        .map(|i| {
            let p = 2 * ((i / W) * width + i % W);
            let [a, b, c, d] = [p, p + 1, p + width, p + width + 1];
            let mut best = (0i32, rgb[p]);
            for (a, b) in [(a, b), (c, d), (a, c), (b, d)] {
                let (u, v) = (i32::from(signal[a]), i32::from(signal[b]));
                let peak = u.min(v).max(0) - (-u).min(-v).max(0);
                if peak.abs() > best.0.abs() {
                    best = (
                        peak,
                        std::array::from_fn(|c| {
                            u16::midpoint(rgb[a][c].into(), rgb[b][c].into()) as u8
                        }),
                    );
                }
            }
            best
        })
        .collect()
}

/// Measures line and spot responses over the valid part of each row.
fn ridge_responses(levels: &[[i32; 3]], fine: &mut [i16], contrast: &mut [i16]) {
    for (y, (fine, contrast)) in
        fine.chunks_exact_mut(W).zip(contrast.chunks_exact_mut(W)).enumerate()
    {
        for (step, penalty) in [(2, 0), (3, 1), (5, 4), (7, 8)] {
            for (dx, dy) in
                [(step, 0), (0, step as isize), (step, step as isize), (step, -(step as isize))]
            {
                let (Some(top), Some(bottom)) =
                    (y.checked_add_signed(-dy), y.checked_add_signed(dy))
                else {
                    continue;
                };
                if top >= H || bottom >= H {
                    continue;
                }
                let centers = &levels[y * W + dx..(y + 1) * W - dx];
                let before = &levels[top * W..(top + 1) * W - 2 * dx];
                let after = &levels[bottom * W + 2 * dx..(bottom + 1) * W];
                let fine = &mut fine[dx..W - dx];
                let contrast = &mut contrast[dx..W - dx];
                for ((((short, long), center), before), after) in
                    fine.iter_mut().zip(contrast).zip(centers).zip(before).zip(after)
                {
                    let mut small = i32::from(*short);
                    let mut strongest = i32::from(*long);
                    for c in 0..3 {
                        let (a, b) = (center[c] - before[c], center[c] - after[c]);
                        let signal = (a.min(b) - penalty).max(0) - ((-a).min(-b) - penalty).max(0);
                        if step <= 3 && signal.abs() > small.abs() {
                            small = signal;
                        }
                        if signal.abs() > strongest.abs() {
                            strongest = signal;
                        }
                    }
                    *short = small as i16;
                    *long = strongest as i16;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Equal dark and light responses preserve the first direction's sign.
    #[test]
    fn keeps_first_dense_contrast_direction_on_ties() {
        let width = DETAIL_PLANE.width;
        let mut dense = vec![[255; 3]; width * DETAIL_PLANE.height];
        let i = 8 * width + 8;
        dense[i] = [128; 3];
        dense[i - 3] = [1; 3];
        dense[i + 3] = [1; 3];
        assert_eq!(dense_contrast(&dense)[i], 127);
    }

    /// A dark mark on a light ramp survives while the ramp produces almost no local response.
    #[test]
    fn finds_compact_detail_under_smooth_lighting() {
        let mut luma: Vec<i32> = (0..W * H).map(|i| 50 + (i % W / 2) as i32).collect();
        for y in 43..47 {
            for x in 90..103 {
                luma[y * W + x] -= 45;
            }
        }
        let rgb: Vec<_> = luma.iter().map(|&v| [v as u8; 3]).collect();
        let detail = Detail::new(&luma, &rgb, &[]);
        assert!(detail.contrast[44 * W + 96] < -35);
        assert!(detail.span(45, 20, 80) < 60);
    }

    /// Gives coarse marks priority only when the active region already resolves them.
    #[test]
    fn reserves_fine_slots_for_features_missed_by_coarse_sampling() {
        let mut detail = Detail::new(&vec![120; W * H], &vec![[120; 3]; W * H], &[]);
        detail.samples.resize(W * H, None);
        let area = W * H / 8;
        let subject: Vec<_> = (0..W * H).map(|i| i < area).collect();
        let marks = area / 5;
        detail.fine[..marks].fill(-24);
        assert!(detail.supports_coarse_marks(&subject));

        detail.samples[..marks].fill(Some([96; 3]));
        assert!(!detail.supports_coarse_marks(&subject));
        detail.fine[area..area + marks].fill(-24);
        assert!(!detail.supports_coarse_marks(&subject));
    }

    /// A thin dark stroke missed by the average can use two agreeing samples at twice the resolution.
    #[test]
    fn preserves_a_subcell_stroke() {
        let mut dense = vec![[160; 3]; DETAIL_PLANE.width * DETAIL_PLANE.height];
        for x in 110..150 {
            dense[91 * DETAIL_PLANE.width + x] = [84; 3];
        }
        let rgb = crate::layers::fit::sample_plane(DETAIL_PLANE, |x, y| {
            dense[y * DETAIL_PLANE.width + x]
        });
        let levels: Vec<_> = rgb.iter().copied().map(luma).collect();
        let compare = Detail::new(&levels, &rgb, &[]);
        let detail = Detail::new(&levels, &rgb, &dense);
        let i = 45 * W + 65;
        assert!(detail.fine[i] < compare.fine[i] - 30);
        assert_eq!(detail.samples[i], Some([84; 3]));
    }
}
