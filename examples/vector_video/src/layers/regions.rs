//! Connected color bands and local-contrast islands supply nested contour proposals.
use super::{Canvas, Outline, DETAIL_PLANE, H, PLANE, VERTICES, W};
#[cfg(any(test, feature = "object-layers"))]
use std::ops::Range;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashSet},
};
const STRIDE: usize = W + 2;
type GridPoint = [u16; 2];
/// Differences under 12 RGB levels per channel on average can share one flat fill.
pub(super) const SIMILAR_COLOR_ERROR: u32 = 3 * 12 * 12;

/// The largest deviation inside one chord of a progressively simplified outline.
#[derive(Clone, Copy, Default)]
struct Chord {
    start: usize,
    end: usize,
    split: usize,
    error: f32,
}

impl Chord {
    /// Finds the first farthest point between the endpoints of this chord.
    fn fit(ring: &[GridPoint], start: usize, end: usize) -> Self {
        let [x, y] = ring[start].map(f32::from);
        let [ex, ey] = ring[end % ring.len()].map(f32::from);
        let (dx, dy) = (ex - x, ey - y);
        let scale = (dx * dx + dy * dy).max(1.0);
        let mut chord = Self { start, end, ..Self::default() };
        for (i, &[px, py]) in ring.iter().enumerate().take(end).skip(start + 1) {
            let (qx, qy) = (f32::from(px) - x, f32::from(py) - y);
            let t = ((dx * qx + dy * qy) / scale).clamp(0.0, 1.0);
            let error = (qx - t * dx).powi(2) + (qy - t * dy).powi(2);
            if error > chord.error {
                chord.error = error;
                chord.split = i;
            }
        }
        chord
    }
}

/// Simplifies a closed contour by repeatedly retaining its largest departure from a chord.
fn simplify(ring: &[GridPoint], grid: Canvas) -> Outline {
    let mut chords = [Chord::default(); VERTICES];
    chords[0] = Chord::fit(ring, 0, ring.len());
    let mut count = 1;
    while count < VERTICES {
        // Reverse iteration makes max_by prefer the first chord in the event of a tie.
        let Some((i, &chord)) = chords[..count]
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, chord)| chord.error > 0.24)
            .max_by(|(_, a), (_, b)| a.error.total_cmp(&b.error))
        else {
            break;
        };
        chords.copy_within(i + 1..count, i + 2);
        chords[i] = Chord::fit(ring, chord.start, chord.split);
        chords[i + 1] = Chord::fit(ring, chord.split, chord.end);
        count += 1;
    }
    std::array::from_fn(|i| {
        let [x, y] = ring[chords[i.min(count - 1)].start];
        normalized_point(x, y, grid)
    })
}

/// Rounds a raster boundary vertex to the full wire coordinate range.
fn normalized_point(x: u16, y: u16, grid: Canvas) -> [u8; 2] {
    [
        ((usize::from(x) * 255 + grid.width / 2) / grid.width) as u8,
        ((usize::from(y) * 255 + grid.height / 2) / grid.height) as u8,
    ]
}

/// Walks the outer pixel boundary with the component on the right, leaving holes for later layers.
fn trace<T: Copy + Eq>(
    labels: &[T],
    seed: usize,
    area: usize,
    ring: &mut Vec<GridPoint>,
    grid: Canvas,
) -> Option<Outline> {
    trace_with_coverage(labels, seed, area, ring, grid, false)
}

/// Traces the component and optionally selects the quantized outline by raster coverage.
fn trace_with_coverage<T: Copy + Eq>(
    labels: &[T],
    seed: usize,
    area: usize,
    ring: &mut Vec<GridPoint>,
    grid: Canvas,
    compare_coverage: bool,
) -> Option<Outline> {
    let stride = grid.width + 2;
    let start = [seed % stride - 1, seed / stride - 1];
    let (mut x, mut y, mut dir) = (start[0], start[1], 0usize);
    let key = labels[seed];
    ring.clear();
    ring.push([x as u16, y as u16]);
    for _ in 0..=4 * area {
        match dir {
            0 => x += 1,
            1 => y += 1,
            2 => x -= 1,
            _ => y -= 1,
        }
        let pos = (y + 1) * stride + x + 1;
        let corners = [pos, pos - 1, pos - 1 - stride, pos - stride];
        let next = [(dir + 1) % 4, dir, (dir + 3) % 4]
            .into_iter()
            .find(|&d| labels[corners[d]] == key && labels[corners[(d + 3) % 4]] != key)?;
        if [x, y] == start && next == 0 {
            return (ring.len() >= 3).then(|| {
                if compare_coverage && ring.len() > VERTICES {
                    best_coverage(ring, grid, labels, key)
                } else {
                    simplify(ring, grid)
                }
            });
        }
        if next != dir {
            ring.push([x as u16, y as u16]);
        }
        dir = next;
    }
    None
}

/// Removes the vertices with smallest effective area until the wire outline fits.
fn simplify_by_area(ring: &[GridPoint], grid: Canvas) -> Outline {
    use std::{cmp::Reverse, collections::BinaryHeap};

    let n = ring.len();
    let mut links: Vec<_> = (0..n).map(|i| ((i + n - 1) % n, (i + 1) % n)).collect();
    let triangle_area = |i: usize, links: &[(usize, usize)]| {
        let (left, right) = links[i];
        let [x, y] = ring[i].map(i32::from);
        let [a, b] = ring[left].map(i32::from);
        let [c, d] = ring[right].map(i32::from);
        ((a - x) * (d - y) - (c - x) * (b - y)).unsigned_abs()
    };
    let mut versions = vec![0; n];
    let mut pending: BinaryHeap<_> =
        (0..n).map(|i| Reverse((triangle_area(i, &links), i, 0))).collect();
    for _ in VERTICES..n {
        while let Some(Reverse((priority, i, version))) = pending.pop() {
            if version != versions[i] {
                continue;
            }
            let (left, right) = links[i];
            links[left].1 = right;
            links[right].0 = left;
            versions[i] = usize::MAX;
            for neighbor in [left, right] {
                versions[neighbor] += 1;
                let area = triangle_area(neighbor, &links).max(priority);
                pending.push(Reverse((area, neighbor, versions[neighbor])));
            }
            break;
        }
    }
    let mut remaining = ring
        .iter()
        .zip(versions)
        .filter_map(|(point, version)| (version != usize::MAX).then_some(point));
    let mut last = [0, 0];
    std::array::from_fn(|_| {
        if let Some(&[x, y]) = remaining.next() {
            last = normalized_point(x, y, grid);
        }
        last
    })
}

/// Scores an outline by counting covered component pixels and subtracting spill.
fn coverage_score<T: Copy + Eq>(outline: &Outline, grid: Canvas, labels: &[T], key: T) -> i32 {
    let stride = grid.width + 2;
    let edges = super::Edges::new(outline, grid);
    let mut score = 0;
    for y in edges.top..edges.bottom {
        let row = (y + 1) * stride + 1;
        edges.spans(y, |left, right| {
            for &label in &labels[row + left..row + right] {
                score += if label == key { 1 } else { -1 };
            }
        });
    }
    score
}

/// Chooses among outlines using coverage after rounding to the wire coordinates.
fn best_coverage<T: Copy + Eq>(
    ring: &mut [GridPoint],
    grid: Canvas,
    labels: &[T],
    key: T,
) -> Outline {
    let fit = |outline: &Outline| coverage_score(outline, grid, labels, key);
    let mut best = simplify(ring, grid);
    let mut best_score = fit(&best);
    let mut consider = |outline| {
        let score = fit(&outline);
        if score > best_score {
            best_score = score;
            best = outline;
        }
    };
    consider(simplify_by_area(ring, grid));
    let shift = ring.len() / 3;
    for _ in 0..2 {
        ring.rotate_left(shift);
        consider(simplify(ring, grid));
    }
    best
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    /// Matches the reference flood fill for connectivity, seed order, area, and strength.
    #[test]
    fn scans_the_same_components_as_flood_fill() {
        let width = 21;
        let height = 14;
        let stride = width + 2;
        let [left, top, right, bottom] = [2, 3, 19, 13];
        let mut components = Components::new(width, [left, top, right, bottom]);
        let mut state = 19u32;
        let weights: Vec<_> = (0..width * height).map(|i| i % 17).collect();
        for case in 0..40 {
            let mut labels = vec![0u8; (height + 2) * stride];
            for y in top + 1..=bottom {
                for x in left + 1..=right {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    labels[y * stride + x] = ((state >> 16) % (2 + case % 5)) as u8;
                }
            }
            let strength = |start, len| weights[start..start + len].iter().sum::<usize>();
            let mut actual = Vec::new();
            components.visit(
                &labels,
                |label| label != 0,
                strength,
                |seed, area, sum| {
                    actual.push((seed, area, sum));
                },
            );
            let mut expected = Vec::new();
            let mut visited = vec![false; labels.len()];
            let mut runs = Vec::new();
            for y in top + 1..=bottom {
                for x in left + 1..=right {
                    let seed = y * stride + x;
                    if labels[seed] != 0 && !visited[seed] {
                        let area = flood(&labels, &mut visited, seed, &mut runs, stride);
                        let sum = runs
                            .iter()
                            .map(|run| {
                                let start =
                                    (run.start / stride - 1) * width + run.start % stride - 1;
                                strength(start, run.len())
                            })
                            .sum();
                        expected.push((seed, area, sum));
                    }
                }
            }
            assert_eq!(actual, expected);
        }
    }

    /// The fixed-width outline can cover a narrow bend and wider base with less spill.
    #[test]
    fn chooses_a_better_outline_for_a_bent_component() {
        let mut labels = vec![0; (H + 2) * STRIDE];
        let mut area = 0;
        for y in 15..73 {
            let (left, right) = if y < 47 {
                let x = 68 + (y - 15) / 2;
                (x, x + 2)
            } else {
                let row = y - 47;
                (26 + (row / 4 % 4) * 6, 123 - (row / 2 % 5) * 6)
            };
            let row = (y + 1) * STRIDE + 1;
            labels[row + left..row + right].fill(1u8);
            area += right - left;
        }
        let seed = 16 * STRIDE + 69;
        let mut ring = Vec::new();
        let ordinary = trace(&labels, seed, area, &mut ring, PLANE).unwrap();
        let fitted = trace_with_coverage(&labels, seed, area, &mut ring, PLANE, true).unwrap();
        assert!(
            coverage_score(&fitted, PLANE, &labels, 1)
                > coverage_score(&ordinary, PLANE, &labels, 1)
        );
    }

    /// Continuous source color can reach its own boundary a few pixels beyond the model.
    #[test]
    fn allows_a_color_region_to_reach_a_nearby_image_edge() {
        let model_edge = 70;
        let source_edge = 72;
        let (rgb, subject): (Vec<_>, Vec<_>) = (0..W * H)
            .map(|i| {
                let x = i % W;
                (if x < source_edge { [180; 3] } else { [70; 3] }, x < model_edge)
            })
            .unzip();
        let soft = soft_model_boundary(&rgb, &subject);
        let sample = |x| H / 2 * W + x;
        assert!(!soft[sample(model_edge - 2)]);
        assert!(soft[sample(model_edge - 1)]);
        assert!(soft[sample(source_edge - 1)]);
        assert!(!soft[sample(source_edge + 1)]);

        let aligned: Vec<_> = (0..W * H).map(|i| i % W < source_edge).collect();
        assert!(soft_model_boundary(&rgb, &aligned).iter().all(|&pixel| !pixel));
        let wider: Vec<_> = (0..W * H).map(|i| i % W < source_edge + 2).collect();
        assert!(soft_model_boundary(&rgb, &wider).iter().all(|&pixel| !pixel));

        let padded = |x| (H / 2 + 1) * STRIDE + 1 + x;
        let mut found = false;
        color_regions(&rgb, &subject, true, &soft, |_, seed, region| {
            if seed == STRIDE + 1 {
                found = true;
                assert!(region.contains(&padded(source_edge - 1)));
                assert!(!region.contains(&padded(source_edge)));
            }
        });
        assert!(found);
    }
}

/// Marks the horizontal run containing an unvisited component pixel.
#[cfg(any(test, feature = "object-layers"))]
fn mark_run(labels: &[u8], visited: &mut [bool], seed: usize, key: u8) -> Range<usize> {
    let (mut start, mut end) = (seed, seed + 1);
    while labels[start - 1] == key {
        start -= 1;
    }
    while labels[end] == key {
        end += 1;
    }
    visited[start..end].fill(true);
    start..end
}

/// Counts a component by visiting overlapping runs in the rows above and below each run.
#[cfg(any(test, feature = "object-layers"))]
fn flood(
    labels: &[u8],
    visited: &mut [bool],
    seed: usize,
    runs: &mut Vec<Range<usize>>,
    stride: usize,
) -> usize {
    let key = labels[seed];
    runs.clear();
    runs.push(mark_run(labels, visited, seed, key));
    let mut area = 0;
    let mut head = 0;
    while head < runs.len() {
        let run = runs[head].clone();
        area += run.len();
        head += 1;
        for row in [run.start - stride..run.end - stride, run.start + stride..run.end + stride] {
            let mut i = row.start;
            while i < row.end {
                if !visited[i] && labels[i] == key {
                    let next = mark_run(labels, visited, i, key);
                    i = next.end;
                    runs.push(next);
                } else {
                    i += 1;
                }
            }
        }
    }
    area
}

/// A same-label interval and the earliest interval in its connected component.
struct ComponentRun {
    start: usize,
    end: usize,
    root: usize,
    area: usize,
    strength: usize,
    label: u8,
}

/// Reuses storage while scanning runs in a window of the padded label grid.
struct Components {
    width: usize,
    bounds: [usize; 4],
    runs: Vec<ComponentRun>,
}

impl Components {
    /// Creates a scanner for a window with exclusive right and bottom edges.
    fn new(width: usize, bounds: [usize; 4]) -> Self {
        Self { width, bounds, runs: Vec::new() }
    }

    /// Visits four-connected components in order of their first raster pixel.
    fn visit(
        &mut self,
        labels: &[u8],
        active: impl Fn(u8) -> bool,
        strength: impl Fn(usize, usize) -> usize,
        mut emit: impl FnMut(usize, usize, usize),
    ) {
        let [left, top, right, bottom] = self.bounds;
        let stride = self.width + 2;
        let runs = &mut self.runs;
        runs.clear();
        let mut previous = 0..0;
        for y in top + 1..=bottom {
            let row = y * stride;
            let first = runs.len();
            let mut x = left + 1;
            while x <= right {
                let start = x;
                let label = labels[row + x];
                x += 1;
                while x <= right && labels[row + x] == label {
                    x += 1;
                }
                if active(label) {
                    let area = x - start;
                    let pixel = (y - 1) * self.width + start - 1;
                    runs.push(ComponentRun {
                        start: row + start,
                        end: row + x,
                        root: runs.len(),
                        area,
                        strength: strength(pixel, area),
                        label,
                    });
                }
            }
            let current = first..runs.len();
            let (mut upper, mut lower) = (previous.start, current.start);
            while upper < previous.end && lower < current.end {
                let (a, b) = (runs[upper].start + stride, runs[upper].end + stride);
                let (c, d) = (runs[lower].start, runs[lower].end);
                if a < d && c < b && runs[upper].label == runs[lower].label {
                    let u = component_root(runs, upper);
                    let l = component_root(runs, lower);
                    runs[u.max(l)].root = u.min(l);
                }
                upper += usize::from(b <= d);
                lower += usize::from(d <= b);
            }
            previous = current;
        }

        // Parent indices always point backward, so each parent already names the final root.
        for i in 0..runs.len() {
            let parent = runs[i].root;
            if parent != i {
                let root = runs[parent].root;
                runs[i].root = root;
                runs[root].area += runs[i].area;
                runs[root].strength += runs[i].strength;
            }
        }
        for (i, run) in runs.iter().enumerate() {
            if run.root == i {
                emit(run.start, run.area, run.strength);
            }
        }
    }
}

/// Finds the earliest interval in a component and shortens the parent chain.
fn component_root(runs: &mut [ComponentRun], mut i: usize) -> usize {
    while runs[i].root != i {
        let parent = runs[i].root;
        runs[i].root = runs[parent].root;
        i = parent;
    }
    i
}

/// Visits connected runs of similar color, allowing nearby source-supported boundary crossings.
fn color_regions(
    rgb: &[[u8; 3]],
    subject: &[bool],
    continuous: bool,
    soft_boundary: &[bool],
    mut visit: impl FnMut(&[u16], usize, &[usize]),
) {
    let mut labels = vec![u16::MAX; (H + 2) * STRIDE];
    for row in labels[STRIDE..(H + 1) * STRIDE].chunks_exact_mut(STRIDE) {
        row[1..=W].fill(0);
    }
    let threshold = if continuous { 3 * 5 * 5 } else { SIMILAR_COLOR_ERROR };
    let mut region = Vec::new();
    let mut key = 0;
    for y in 1..=H {
        for x in 1..=W {
            let seed = y * STRIDE + x;
            if labels[seed] != 0 {
                continue;
            }
            let color = rgb[plane_index(seed)];
            let class = subject.get(plane_index(seed));
            key += 1;
            labels[seed] = key;
            region.clear();
            region.push(seed);
            let mut head = 0;
            while let Some(&i) = region.get(head) {
                head += 1;
                let reference = if continuous { rgb[plane_index(i)] } else { color };
                for neighbor in [i - 1, i + 1, i - STRIDE, i + STRIDE] {
                    if labels[neighbor] == 0
                        && (subject.get(plane_index(neighbor)) == class
                            || (soft_boundary.get(plane_index(i)) == Some(&true)
                                && soft_boundary.get(plane_index(neighbor)) == Some(&true)))
                        && crate::palette::distance(reference, rgb[plane_index(neighbor)])
                            <= threshold
                    {
                        labels[neighbor] = key;
                        region.push(neighbor);
                    }
                }
            }
            visit(&labels, seed, &region);
        }
    }
}

/// Maps a cell in the padded contour raster to its unpadded pixel index.
fn plane_index(i: usize) -> usize {
    (i / STRIDE - 1) * W + i % STRIDE - 1
}

/// Lets source colors reach a nearby image edge just beyond a weak model boundary.
pub(super) fn soft_model_boundary(rgb: &[[u8; 3]], subject: &[bool]) -> Vec<bool> {
    use crate::palette::distance;

    const SEARCH: usize = 3;
    const WEAK_EDGE: u32 = 3 * 8 * 8;
    const STRONG_EDGE: u32 = 3 * 16 * 16;
    const MARGIN: u32 = 3 * 12 * 12;
    let mut soft = vec![false; rgb.len()];

    for (length, lines, step, stride) in [(W, H, 1, W), (H, W, W, 1)] {
        for line in 1..lines - 1 {
            let start = line * stride;
            let contrast = |i| distance(rgb[i], rgb[i - step]);
            for at in 1..length - 1 {
                let i = start + at * step;
                if subject[i] == subject[i - step] {
                    continue;
                }
                let current = contrast(i);
                if current > WEAK_EDGE {
                    continue;
                }
                let mut best = (STRONG_EDGE.max(current + MARGIN), at);
                // The subject interior can contain strong lighting and texture edges.
                let (first, last) = if subject[i] {
                    (at.saturating_sub(SEARCH).max(1), at - 1)
                } else {
                    (at + 1, (at + SEARCH).min(length - 1))
                };
                for probe in first..=last {
                    let movement = probe.abs_diff(at) as u32;
                    let score = contrast(start + probe * step).saturating_sub(55 * movement);
                    if score > best.0 {
                        best = (score, probe);
                    }
                }
                if best.1 != at {
                    for between in (at.min(best.1) - 1)..=at.max(best.1) {
                        soft[start + between * step] = true;
                    }
                }
            }
        }
    }
    soft
}

/// Flattens broad, smooth surfaces into light and shadow while keeping local marks.
pub(super) fn simplify_tones(
    rgb: &[[u8; 3]],
    luma: &[i32],
    subject: &[bool],
    fine: &[i16],
    shaded: &mut [bool],
    textured: bool,
) -> Vec<[u8; 3]> {
    let mut tones = rgb.to_vec();
    color_regions(rgb, subject, true, &[], |_, seed, region| {
        if region.len() < 120 {
            return;
        }
        let mut hist = [0usize; 256];
        let mut count = 0;
        for i in region.iter().copied().map(plane_index) {
            if fine[i].abs() < 10 {
                hist[luma[i] as usize] += 1;
                count += 1;
            }
        }
        if count < 90 || count * 2 < region.len() {
            return;
        }
        let percentile = |point| {
            let mut seen = 0;
            hist.iter()
                .position(|&v| {
                    seen += v;
                    seen > point
                })
                .unwrap_or(255) as i32
        };
        let low = percentile(count / 10);
        let high = percentile(count * 9 / 10);
        let split = if high - low < 22 { 256 } else { (high + low + 1) / 2 };
        let visible_shading = !shaded.is_empty()
            && high - low >= 32
            && (!textured
                || (count >= 1200
                    && subject.get(plane_index(seed)) == Some(&true)
                    && coherent_lighting(region, luma, fine, count)));
        let mut sums = [[0u32; 4]; 2];
        for i in region.iter().copied().map(plane_index) {
            if fine[i].abs() < 10 {
                let row = &mut sums[usize::from(luma[i] >= split)];
                for (sum, value) in row.iter_mut().zip(rgb[i]) {
                    *sum += u32::from(value);
                }
                row[3] += 1;
            }
        }
        let shades = sums.map(|[r, g, b, n]| [r, g, b].map(|sum| ((sum + n / 2) / n.max(1)) as u8));
        for i in region.iter().copied().map(plane_index) {
            if fine[i].abs() < 10 {
                tones[i] = shades[usize::from(luma[i] >= split)];
                if visible_shading {
                    shaded[i] = true;
                }
            }
        }
    });
    tones
}

/// Checks whether one smooth lighting direction explains most of a region's luminance variation.
fn coherent_lighting(region: &[usize], luma: &[i32], fine: &[i16], count: usize) -> bool {
    let mut sums = [0i64; 9];
    for i in region.iter().copied().map(plane_index) {
        if fine[i].abs() < 10 {
            let (x, y, light) = ((i % W) as i64, (i / W) as i64, i64::from(luma[i]));
            for (sum, term) in sums.iter_mut().zip([
                x,
                y,
                light,
                x * x,
                y * y,
                light * light,
                x * y,
                x * light,
                y * light,
            ]) {
                *sum += term;
            }
        }
    }
    let [x, y, light, xx, yy, ll, xy, xl, yl] = sums.map(|sum| sum as f64);
    let n = count as f64;
    let (xx, yy, ll) = (xx - x * x / n, yy - y * y / n, ll - light * light / n);
    let (xy, xl, yl) = (xy - x * y / n, xl - x * light / n, yl - y * light / n);
    // Solve the two normal equations for a plane, with the determinant factored out.
    let total = ll * (xx * yy - xy * xy);
    let explained = yy * xl * xl + xx * yl * yl - 2.0 * xy * xl * yl;
    total > 0.0 && explained >= 0.85 * total
}

/// Extracts proposals from color and contrast within the optional foreground partition.
pub(super) fn proposals(
    rgb: &[[u8; 3]],
    source: &[[u8; 3]],
    local: &[i16],
    subject: &[bool],
    soft_boundary: &[bool],
    detail: Option<&super::detail::Detail>,
) -> Vec<Outline> {
    let mut labels = vec![255; (H + 2) * STRIDE];
    let mut components = Components::new(W, [0, 0, W, H]);
    let mut ring = Vec::new();
    let mut unique = HashSet::new();
    let mut result = Vec::new();
    let mut emit = |outline| {
        if unique.insert(outline) {
            result.push(outline);
        }
    };
    let compare_coverage = detail.is_some_and(|detail| detail.strong_edges < W * H / 5);
    for continuous in [false, true] {
        if continuous && detail.is_none() {
            break;
        }
        let input = if continuous { source } else { rgb };
        let soft = if continuous { soft_boundary } else { &[] };
        color_regions(input, subject, continuous, soft, |labels, seed, region| {
            let minimum = if continuous { 24 } else { 4 };
            if (minimum..W * H * 15 / 16).contains(&region.len()) {
                if let Some(outline) = trace_with_coverage(
                    labels,
                    seed,
                    region.len(),
                    &mut ring,
                    PLANE,
                    compare_coverage,
                ) {
                    emit(outline);
                }
            }
        });
    }
    const SUBJECT_LABEL: u8 = 1 << 7;
    const SUBJECT_CHANNEL: u16 = (SUBJECT_LABEL as u16) << 8;
    // Every channel fits in 0..=510, leaving the high bit for the subject split.
    let channels: Vec<_> = rgb
        .iter()
        .zip(local)
        .enumerate()
        .map(|(i, (&[r, g, b], &local))| {
            let [r, g, b] = [r, g, b].map(u16::from);
            let class = u16::from(subject.get(i) == Some(&true)) * SUBJECT_CHANNEL;
            [
                ((r + 2 * g + b) / 4) | class,
                (255 + r - b) | class,
                (255 + g - (r + b) / 2) | class,
                ((255 + local) as u16) | class,
            ]
        })
        .collect();
    for (channel, step, shift) in [
        (0, 80, 0),
        (0, 80, 40),
        (0, 40, 0),
        (0, 40, 20),
        (0, 20, 0),
        (1, 32, 0),
        (1, 32, 16),
        (1, 16, 0),
        (2, 32, 0),
        (2, 32, 16),
        (2, 16, 0),
        (3, 20, 10),
        (3, 10, 5),
    ] {
        let buckets: [u8; 511] = std::array::from_fn(|i| ((i + shift) / step) as u8);
        let rows = labels[STRIDE..(H + 1) * STRIDE].chunks_exact_mut(STRIDE);
        for (row, channels) in rows.zip(channels.chunks_exact(W)) {
            for (label, values) in row[1..=W].iter_mut().zip(channels) {
                let value = values[channel];
                let class = (value >> 8) as u8 & SUBJECT_LABEL;
                *label = buckets[(value & !SUBJECT_CHANNEL) as usize] | class;
            }
        }
        components.visit(
            &labels,
            |_| true,
            |_, _| 0,
            |seed, area, _| {
                if (4..W * H * 15 / 16).contains(&area) {
                    if let Some(outline) =
                        trace_with_coverage(&labels, seed, area, &mut ring, PLANE, compare_coverage)
                    {
                        emit(outline);
                    }
                }
            },
        );
    }
    if let Some(detail) = detail {
        for (contrast, threshold, max_area) in [
            (&detail.fine, 6, W * H / 50),
            (&detail.fine, 12, W * H / 50),
            (&detail.contrast, 12, W * H / 22),
            (&detail.contrast, 24, W * H / 22),
        ] {
            let rows = labels[STRIDE..(H + 1) * STRIDE].chunks_exact_mut(STRIDE);
            for ((row, signal), channels) in
                rows.zip(contrast.chunks_exact(W)).zip(channels.chunks_exact(W))
            {
                for ((label, &v), values) in row[1..=W].iter_mut().zip(signal).zip(channels) {
                    let kind = u8::from(v >= threshold) + 2 * u8::from(v <= -threshold);
                    let class = (values[0] >> 8) as u8 & SUBJECT_LABEL;
                    *label = kind | class;
                }
            }
            components.visit(
                &labels,
                |label| label & 3 != 0,
                |_, _| 0,
                |seed, area, _| {
                    if (3..max_area).contains(&area) {
                        if let Some(outline) = trace(&labels, seed, area, &mut ring, PLANE) {
                            emit(outline);
                        }
                    }
                },
            );
        }
    }
    result
}

/// Shortlists connected components before performing the more expensive contour trace.
fn shortlist(
    grid: Canvas,
    components: &mut Components,
    labels: &[u8],
    limit: usize,
    rank: impl Fn(usize, usize) -> usize,
    mut emit: impl FnMut(Outline),
) {
    let mut top = BinaryHeap::with_capacity(limit + 1);
    components.visit(
        labels,
        |key| key < 255,
        |_, _| 0,
        |seed, area, _| {
            let score = rank(seed, area);
            if score == 0 {
                return;
            }
            let item = (score, Reverse(seed), area);
            if top.len() < limit {
                top.push(Reverse(item));
            } else if let Some(mut low) = top.peek_mut() {
                if item > low.0 {
                    *low = Reverse(item);
                }
            }
        },
    );
    let mut ring = Vec::new();
    for Reverse((_, Reverse(seed), area)) in top.into_sorted_vec() {
        if let Some(outline) = trace(labels, seed, area, &mut ring, grid) {
            emit(outline);
        }
    }
}

/// Finds broad bands on a smaller grid and traces a bounded set of fine marks at full analysis resolution.
pub(super) fn budgeted_proposals(
    rgb: &[[u8; 3]],
    subject: &[bool],
    detail: Option<&[i16]>,
) -> Vec<Outline> {
    const GRID: Canvas = Canvas { width: W / 2, height: H / 2 };
    const SUBJECT: u8 = 128;
    let Canvas { width, height } = GRID;
    let stride = width + 2;
    let mut labels = vec![255; (height + 2) * stride];
    let mut components = Components::new(width, [0, 0, width, height]);
    let mut seen = HashSet::new();
    let mut proposals = Vec::new();
    let mut emit = |outline| {
        if seen.insert(outline) {
            proposals.push(outline);
        }
    };
    let mut bands = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let i = 2 * (y * W + x);
            let [r, g, b]: [u16; 3] = std::array::from_fn(|c| {
                [i, i + 1, i + W, i + W + 1].into_iter().map(|j| u16::from(rgb[j][c])).sum::<u16>()
                    / 4
            });
            let class = SUBJECT * u8::from(subject.get(i) == Some(&true));
            let light = (r + 2 * g + b) / 4;
            let red_blue = 255 + r - b;
            let green = 255 + g - (r + b) / 2;
            bands.push((
                class,
                [light, red_blue, green],
                ((r >> 6) << 4 | (g >> 6) << 2 | (b >> 6)) as u8,
            ));
        }
    }
    for (channel, step, shift) in
        [(0, 0, 0), (0, 36, 0), (0, 36, 18), (0, 64, 0), (1, 48, 16), (2, 48, 16)]
    {
        for (row, values) in labels[stride..(height + 1) * stride]
            .chunks_exact_mut(stride)
            .zip(bands.chunks_exact(width))
        {
            for (label, &(class, values, color)) in row[1..=width].iter_mut().zip(values) {
                *label = class
                    | (values[channel] + shift).checked_div(step).map_or(color, |band| band as u8);
            }
        }
        shortlist(
            GRID,
            &mut components,
            &labels,
            36,
            |seed, area| {
                if !(3..width * height * 15 / 16).contains(&area) {
                    return 0;
                }
                area * if labels[seed] & SUBJECT != 0 { 2 } else { 1 }
            },
            &mut emit,
        );
    }
    if let Some(detail) = detail {
        let mut labels = vec![255; (H + 2) * STRIDE];
        let mut components = Components::new(W, [0, 0, W, H]);
        for (y, row) in labels[STRIDE..(H + 1) * STRIDE].chunks_exact_mut(STRIDE).enumerate() {
            for (x, label) in row[1..=W].iter_mut().enumerate() {
                let i = y * W + x;
                let strength = detail[i];
                *label = if strength.abs() >= 10 {
                    2 * u8::from(strength < 0) + u8::from(subject.get(i) == Some(&true))
                } else {
                    255
                };
            }
        }
        shortlist(
            PLANE,
            &mut components,
            &labels,
            104,
            |seed, area| {
                if !(2..=250).contains(&area) {
                    return 0;
                }
                let strength = usize::from(detail[plane_index(seed)].unsigned_abs());
                strength * area.isqrt() * if labels[seed] & 1 != 0 { 2 } else { 1 }
            },
            emit,
        );
    }
    proposals
}

/// Traces light and dark marks on the fine grid before wire quantization.
pub(super) fn compact_outlines(signal: &[i16], thresholds: &[i16], limit: usize) -> Vec<Outline> {
    compact_outlines_in(signal, thresholds, limit, [0, 0, DETAIL_PLANE.width, DETAIL_PLANE.height])
}

/// Traces compact marks inside the bounds of a sparse signal on the fine grid.
pub(super) fn compact_outlines_in(
    signal: &[i16],
    thresholds: &[i16],
    limit: usize,
    [left, top, right, bottom]: [usize; 4],
) -> Vec<Outline> {
    let Canvas { width, height } = DETAIL_PLANE;
    let stride = width + 2;
    let mut labels = vec![0u8; (height + 2) * stride];
    let mut components = Components::new(width, [left, top, right, bottom]);
    let mut ring = Vec::new();
    let mut unique = HashSet::new();
    let mut traced = Vec::new();
    for &threshold in thresholds {
        for (y, row) in signal.chunks_exact(width).enumerate().take(bottom).skip(top) {
            let offset = (y + 1) * stride + 1 + left;
            for (key, &v) in labels[offset..offset + right - left].iter_mut().zip(&row[left..right])
            {
                *key = u8::from(v >= threshold) + 2 * u8::from(v <= -threshold);
            }
        }
        components.visit(
            &labels,
            |label| label != 0,
            |start, len| signal[start..start + len].iter().map(|v| v.unsigned_abs() as usize).sum(),
            |seed, area, strength| {
                if !(5..=600).contains(&area) {
                    return;
                }
                let Some(outline) = trace(&labels, seed, area, &mut ring, DETAIL_PLANE) else {
                    return;
                };
                if unique.insert(outline) {
                    traced.push((strength / (area + 16).isqrt(), outline));
                }
            },
        );
    }
    traced.sort_by_key(|&(strength, _)| std::cmp::Reverse(strength));
    traced.truncate(limit);
    traced.into_iter().map(|(_, outline)| outline).collect()
}

/// Traces compact light and dark regions around marks missing from the current paint.
pub(super) fn persistent_marks(
    source: &[[i16; 3]],
    current: &[[i16; 3]],
    signal: &[i16],
) -> Vec<Outline> {
    const CELL: usize = 16;
    const MARGIN: usize = 5;
    let Canvas { width, height } = DETAIL_PLANE;
    let mut seeds = Vec::new();
    for sign in [-1i16, 1] {
        for y in (MARGIN..height - MARGIN).step_by(CELL) {
            for x in (MARGIN..width - MARGIN).step_by(CELL) {
                let mut best = (0, 0);
                for v in y..(y + CELL).min(height - MARGIN) {
                    for u in x..(x + CELL).min(width - MARGIN) {
                        let i = v * width + u;
                        let ridge = sign * signal[i];
                        let missing = sign * (source[i][0] - current[i][0]);
                        if ridge >= 4 && missing >= 4 {
                            // Cap the two terms so strong edges leave room for weaker marks.
                            let strength = 2 * ridge.min(24) + missing.min(28);
                            if strength > best.0 {
                                best = (strength, i);
                            }
                        }
                    }
                }
                if best.0 != 0 {
                    seeds.push((best.0, best.1, sign));
                }
            }
        }
    }
    grow_marks(source, seeds)
}

/// Finds a strong light and dark seed within each model-guided region.
#[cfg(feature = "object-layers")]
pub(super) fn contextual_marks(
    source: &[[i16; 3]],
    current: &[[i16; 3]],
    signal: &[i16],
    regions: &[u8],
) -> Vec<Outline> {
    let Canvas { width, height } = DETAIL_PLANE;
    let mut best = [[(0, 0, 0); 2]; super::parts::MAX_MARKS + 1];
    for y in 5..height - 5 {
        for x in 5..width - 5 {
            let i = y * width + x;
            let region = usize::from(regions[i]);
            if region == 0 {
                continue;
            }
            for (side, sign) in [-1, 1].into_iter().enumerate() {
                let ridge = sign * signal[i];
                let missing = sign * (source[i][0] - current[i][0]);
                let strength = 2 * ridge.min(24) + missing.min(27);
                if ridge >= 3 && missing >= 4 && strength > best[region][side].0 {
                    best[region][side] = (strength, i, sign);
                }
            }
        }
    }
    grow_marks(source, best.into_iter().flatten().filter(|seed| seed.0 != 0).collect())
}

/// Traces each seeded mark at several nearby luminance thresholds.
fn grow_marks(source: &[[i16; 3]], mut seeds: Vec<(i16, usize, i16)>) -> Vec<Outline> {
    const RADIUS: usize = 14;
    const MAX_AREA: usize = 220;
    const MAX_SEEDS: usize = 72;
    let Canvas { width, height } = DETAIL_PLANE;
    let stride = width + 2;
    seeds.sort_unstable_by_key(|&(strength, i, sign)| (std::cmp::Reverse(strength), i, sign));
    seeds.truncate(MAX_SEEDS);
    let mut labels = vec![0u8; (height + 2) * stride];
    let mut marked = vec![false; width * height];
    let mut queue = Vec::new();
    let mut ring = Vec::new();
    let mut outlines = Vec::new();
    let mut traced = Vec::new();
    for (_, i, sign) in seeds {
        if marked[i] {
            continue;
        }
        let (x, y) = (i % width, i / width);
        let (left, right) = (x.saturating_sub(RADIUS), (x + RADIUS).min(width - 1));
        let (top, bottom) = (y.saturating_sub(RADIUS), (y + RADIUS).min(height - 1));
        let seed = (y + 1) * stride + x + 1;
        let center = source[i][0];
        for row in y.saturating_sub(3)..=(y + 3).min(height - 1) {
            marked[row * width + x.saturating_sub(3)..=row * width + (x + 3).min(width - 1)]
                .fill(true);
        }
        traced.clear();
        for tolerance in [3, 6, 10, 15, 21, 29] {
            labels[seed] = 1;
            queue.clear();
            queue.push(seed);
            let mut head = 0;
            let mut outside = false;
            while let Some(&p) = queue.get(head) {
                if queue.len() > MAX_AREA || outside {
                    break;
                }
                head += 1;
                for q in [p - 1, p + 1, p - stride, p + stride] {
                    if labels[q] != 0 {
                        continue;
                    }
                    let (u, v) = (q % stride - 1, q / stride - 1);
                    if sign * (center - source[v * width + u][0]) > tolerance {
                        continue;
                    }
                    // A mark must close inside the search window before it becomes a polygon.
                    if u == left || u == right || v == top || v == bottom {
                        outside = true;
                        continue;
                    }
                    labels[q] = 1;
                    queue.push(q);
                }
            }
            let area = queue.len();
            let complete = !outside && area <= MAX_AREA;
            if complete && area >= 5 && traced.last().is_none_or(|&(last, _)| last != area) {
                let first = *queue.iter().min().expect("seeded mark has at least one pixel");
                if let Some(outline) = trace(&labels, first, area, &mut ring, DETAIL_PLANE) {
                    traced.push((area, outline));
                }
            }
            for &q in &queue {
                labels[q] = 0;
            }
            if !complete {
                break;
            }
        }
        if let Some(&(_, outline)) = traced.last() {
            outlines.push(outline);
        }
        // The slowest area growth between levels gives a tighter contour before a merger.
        if let Some(pair) = traced
            .windows(2)
            .filter(|pair| pair[0].1 != pair[1].1)
            .min_by_key(|pair| (pair[1].0 - pair[0].0) * 100 / pair[0].0)
        {
            outlines.push(pair[0].1);
        }
    }
    outlines
}

/// An object's coarse outline and an optional partition that preserves more of its mask.
#[cfg(feature = "object-layers")]
pub(super) struct Silhouette {
    pub(super) outline: Outline,
    pub(super) parts: Vec<Outline>,
    pub(super) extensions: Vec<Outline>,
}

/// Traces the two largest substantial foreground components in the neural mask.
#[cfg(feature = "object-layers")]
pub(super) fn silhouettes(mask: &[bool], refine: bool) -> Vec<Silhouette> {
    if mask.is_empty() {
        return Vec::new();
    }
    let mut labels = vec![0; (H + 2) * STRIDE];
    for (y, row) in mask.chunks_exact(W).enumerate() {
        for (x, &inside) in row.iter().enumerate() {
            labels[(y + 1) * STRIDE + x + 1] = u8::from(inside);
        }
    }
    let mut visited = vec![false; labels.len()];
    let mut runs = Vec::new();
    let mut ring = Vec::new();
    let mut shapes = Vec::new();
    for y in 1..=H {
        for x in 1..=W {
            let seed = y * STRIDE + x;
            if labels[seed] == 0 || visited[seed] {
                continue;
            }
            let area = flood(&labels, &mut visited, seed, &mut runs, STRIDE);
            if (W * H / 200..W * H * 9 / 10).contains(&area) {
                if let Some(outline) = trace(&labels, seed, area, &mut ring, PLANE) {
                    shapes.push((area, outline, runs.clone()));
                }
            }
        }
    }
    shapes.sort_by_key(|(area, _, _)| std::cmp::Reverse(*area));
    shapes.truncate(2);
    shapes
        .into_iter()
        .map(|(area, outline, runs)| {
            let parts = partition(area, &outline, &runs, refine);
            let base = if parts.is_empty() { std::slice::from_ref(&outline) } else { &parts };
            let extensions = if refine { recover_extensions(&runs, base) } else { Vec::new() };
            Silhouette { outline, parts, extensions }
        })
        .collect()
}

/// Recovers connected extremities left out of an eight-vertex outline or its partitions.
#[cfg(feature = "object-layers")]
fn recover_extensions(component: &[Range<usize>], base: &[Outline]) -> Vec<Outline> {
    use super::{Edges, PLANE};

    let mut covered = vec![false; W * H];
    for outline in base {
        let edges = Edges::new(outline, PLANE);
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| covered[y * W + l..y * W + r].fill(true));
        }
    }
    let mut missing = vec![0u8; (H + 2) * STRIDE];
    let mut foreground = vec![false; W * H];
    for pixel in component.iter().cloned().flatten() {
        let i = plane_index(pixel);
        foreground[i] = true;
        missing[pixel] = u8::from(!covered[i]);
    }
    let mut visited = vec![false; missing.len()];
    let mut runs = Vec::new();
    let mut ring = Vec::new();
    let mut extensions = Vec::new();
    for pixel in component.iter().cloned().flatten() {
        if missing[pixel] == 0 || visited[pixel] {
            continue;
        }
        let area = flood(&missing, &mut visited, pixel, &mut runs, STRIDE);
        if area < 15 {
            continue;
        }
        let first = runs.iter().map(|r| r.start).min().expect("nonempty residual component");
        let Some(outline) = trace(&missing, first, area, &mut ring, PLANE) else { continue };
        let edges = Edges::new(&outline, PLANE);
        let mut gain = 0;
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| {
                for i in y * W + l..y * W + r {
                    if !covered[i] {
                        gain += if foreground[i] { 1 } else { -2 };
                    }
                }
            });
        }
        if gain >= 12 {
            extensions.push((gain, outline));
        }
    }
    extensions.sort_by_key(|&(gain, _)| std::cmp::Reverse(gain));
    extensions.truncate(2);
    extensions.into_iter().map(|(_, outline)| outline).collect()
}

#[cfg(all(test, feature = "object-layers"))]
mod extension_tests {
    use super::*;
    use crate::layers::{Edges, PLANE};

    /// A narrow protrusion clipped by the coarse polygon gets a separate contour.
    #[test]
    fn restores_a_thin_silhouette_extension() {
        let component: Vec<_> = (20..90)
            .map(|y| {
                let left = if (36..41).contains(&y) { 15 } else { 44 };
                let row = (y + 1) * STRIDE + 1;
                row + left..row + 130
            })
            .collect();
        let base = simplify(&[[44, 20], [130, 20], [130, 90], [44, 90]], PLANE);
        let recovered = recover_extensions(&component, &[base]);
        assert_eq!(recovered.len(), 1);
        let mut restored = false;
        Edges::new(&recovered[0], PLANE).spans(38, |l, r| restored |= (l..r).contains(&25));
        assert!(restored);
    }
}

/// Finds one horizontal or vertical cut if it substantially improves a simplified silhouette.
#[cfg(feature = "object-layers")]
fn partition(
    area: usize,
    outline: &Outline,
    component: &[Range<usize>],
    refine: bool,
) -> Vec<Outline> {
    use super::{Edges, PLANE};

    let mut prefix = vec![0i32; (W + 1) * H];
    let (mut left, mut top, mut right, mut bottom) = (W, H, 0, 0);
    for run in component {
        let (x, y) = (run.start % STRIDE - 1, run.start / STRIDE - 1);
        let end = x + run.len();
        left = left.min(x);
        top = top.min(y);
        right = right.max(end);
        bottom = bottom.max(y + 1);
        prefix[y * (W + 1) + x + 1] += 1;
        if end < W {
            prefix[y * (W + 1) + end + 1] -= 1;
        }
    }
    for row in prefix.chunks_exact_mut(W + 1) {
        let (mut pixel, mut sum) = (0, 0);
        for value in row {
            pixel += *value;
            sum += pixel;
            *value = sum;
        }
    }
    // The input-only term is constant. Penalize both painting outside and missing foreground.
    let cost = |outline: &Outline| {
        let edges = Edges::new(outline, PLANE);
        let mut error = 0;
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| {
                let row = y * (W + 1);
                error += (r - l) as i32 - 2 * (prefix[row + r] - prefix[row + l]);
            });
        }
        error
    };
    let mut best_cost = cost(outline);
    if best_cost + area as i32 <= 100 {
        return Vec::new();
    }
    let mut best = Vec::new();
    let mut labels = vec![0; (H + 2) * STRIDE];
    let mut visited = vec![false; labels.len()];
    let mut ring = Vec::new();
    let mut runs = Vec::new();
    let mut parts = Vec::with_capacity(4);
    for (vertical, lo, hi) in [(true, left, right), (false, top, bottom)] {
        for fifth in 1..5 {
            let cut = lo + (hi - lo) * fifth / 5;
            for run in component {
                let (x, y) = (run.start % STRIDE - 1, run.start / STRIDE - 1);
                if vertical {
                    let mid = cut.saturating_sub(x).min(run.len()) + run.start;
                    labels[run.start..mid].fill(1);
                    labels[mid..run.end].fill(2);
                } else {
                    labels[run.clone()].fill(if y < cut { 1 } else { 2 });
                }
            }
            visited.fill(false);
            parts.clear();
            for component_run in component {
                for seed in component_run.clone() {
                    if visited[seed] {
                        continue;
                    }
                    let size = flood(&labels, &mut visited, seed, &mut runs, STRIDE);
                    if size >= W * H / 500 {
                        // Flood discovery order need not be spatial order after clipping.
                        let first = runs.iter().map(|r| r.start).min().expect("nonempty component");
                        if let Some(outline) =
                            trace_with_coverage(&labels, first, size, &mut ring, PLANE, refine)
                        {
                            parts.push(outline);
                        }
                    }
                }
            }
            if (2..=4).contains(&parts.len()) {
                let error = parts.iter().map(cost).sum::<i32>() + 64 * (parts.len() as i32 - 1);
                if error < best_cost {
                    best_cost = error;
                    best.clone_from(&parts);
                }
            }
        }
    }
    best
}
