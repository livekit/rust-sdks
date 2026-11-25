//! Coarse-to-fine fitting of persistent paint layers against the current frame.
use super::*;

const EMPTY_OUTLINE: Outline = [[0, 0]; VERTICES];
/// Weighted color error a late fill must save to take a slot from fine features.
const OPTIONAL_FILL_GAIN: i64 = 450_000;

/// Box samples each cell at four interior points to suppress isolated sensor noise.
pub(super) fn sample_plane(size: Canvas, rgb: impl Fn(usize, usize) -> [u8; 3]) -> Vec<[u8; 3]> {
    sample_grid(size, PLANE, rgb)
}

/// Samples an analysis grid at four interior positions per cell.
pub(crate) fn sample_grid(
    size: Canvas,
    grid: Canvas,
    rgb: impl Fn(usize, usize) -> [u8; 3],
) -> Vec<[u8; 3]> {
    let columns: Vec<_> = (0..grid.width)
        .map(|x| [1, 3].map(|dx| (4 * x + dx) * size.width / (4 * grid.width)))
        .collect();
    let mut result = Vec::with_capacity(grid.width * grid.height);
    for y in 0..grid.height {
        let [top, bottom] = [1, 3].map(|dy| (4 * y + dy) * size.height / (4 * grid.height));
        for &[left, right] in &columns {
            let a = rgb(left, top);
            let b = rgb(right, top);
            let c = rgb(left, bottom);
            let d = rgb(right, bottom);
            result.push(std::array::from_fn(|i| {
                ((2 + u16::from(a[i]) + u16::from(b[i]) + u16::from(c[i]) + u16::from(d[i])) / 4)
                    as u8
            }));
        }
    }
    result
}

/// Source statistics for fitting constant-color polygons and scoring local details.
struct Target {
    rgb: Vec<[u8; 3]>,
    source_rgb: Option<Vec<[u8; 3]>>,
    local: Vec<i16>,
    detail: Option<detail::Detail>,
    weights: Vec<i32>,
    prefix: Vec<[i32; 4]>,
    subject: Vec<bool>,
    soft_boundary: Vec<bool>,
    subject_prefix: Vec<[u16; 2]>,
    shade_prefix: Vec<u16>,
}

impl Target {
    /// Estimates local contrast relative to a 13×13 neighborhood and builds row sums.
    fn new(
        rgb: Vec<[u8; 3]>,
        faces: &[faces::Guide],
        outlines: &[Outline],
        dense: Option<&[[u8; 3]]>,
        focus: &[bool],
        texture_threshold: usize,
    ) -> Self {
        let luma: Vec<_> = rgb.iter().copied().map(luma).collect();
        let detail = dense.map(|samples| detail::Detail::new(&luma, &rgb, samples));
        let subject = subject_mask(outlines);
        let soft_boundary = if !subject.is_empty() && detail.is_some() {
            regions::soft_model_boundary(&rgb, &subject)
        } else {
            Vec::new()
        };
        let mut shaded =
            vec![false; if !subject.is_empty() && detail.is_some() { W * H } else { 0 }];
        let (rgb, source_rgb) = if let Some(signal) = &detail {
            let mut tones = regions::simplify_tones(
                &rgb,
                &luma,
                &subject,
                &signal.fine,
                &mut shaded,
                signal.strong_edges >= texture_threshold,
            );
            for (color, &sample) in tones.iter_mut().zip(&signal.samples) {
                if let Some(sample) = sample {
                    *color = sample;
                }
            }
            (tones, Some(rgb))
        } else {
            (rgb, None)
        };
        let track_shading = shaded.iter().any(|&pixel| pixel);
        let mut integral = vec![0i32; (W + 1) * (H + 1)];
        for y in 0..H {
            let mut sum = 0;
            for x in 0..W {
                sum += luma[y * W + x];
                integral[(y + 1) * (W + 1) + x + 1] = integral[y * (W + 1) + x + 1] + sum;
            }
        }
        let boxes: Vec<_> = faces.iter().map(|g| g.bounds()).collect();
        let mut local = Vec::with_capacity(W * H);
        let mut weights = Vec::with_capacity(W * H);
        let mut prefix = Vec::with_capacity((W + 1) * H);
        let mut subject_prefix = Vec::new();
        let mut shade_prefix = Vec::with_capacity(if track_shading { (W + 1) * H } else { 0 });
        for y in 0..H {
            let mut row = [0; 4];
            prefix.push(row);
            let mut classes = [0u16; 2];
            if !subject.is_empty() {
                subject_prefix.push(classes);
            }
            let mut shade = 0;
            if track_shading {
                shade_prefix.push(shade);
            }
            let (t, b) = (y.saturating_sub(6), (y + 7).min(H));
            for x in 0..W {
                let (l, r) = (x.saturating_sub(6), (x + 7).min(W));
                let sum = integral[b * (W + 1) + r]
                    - integral[b * (W + 1) + l]
                    - integral[t * (W + 1) + r]
                    + integral[t * (W + 1) + l];
                let i = y * W + x;
                let contrast = luma[i] - sum / ((b - t) * (r - l)) as i32;
                let in_face =
                    boxes.iter().any(|&[l, t, r, b]| (l..r).contains(&x) && (t..b).contains(&y));
                let is_subject = subject.get(i) == Some(&true);
                let local_weight = detail.as_ref().map_or(1 + (contrast.abs() / 10).min(5), |d| {
                    1 + ((i32::from(d.contrast[i]).abs() / 8).min(6)
                        + (i32::from(d.edges[i]) / 22).min(4)
                        + (contrast.abs() / 24).min(2))
                    .min(9)
                });
                let weight = local_weight
                    * if in_face {
                        4
                    } else {
                        1 + i32::from(is_subject) + i32::from(focus.get(i) == Some(&true))
                    };
                local.push(contrast as i16);
                weights.push(weight);
                for (c, &v) in rgb[i].iter().enumerate() {
                    row[c] += i32::from(v) * weight;
                }
                row[3] += weight;
                prefix.push(row);
                if !subject.is_empty() {
                    classes[usize::from(is_subject)] +=
                        u16::from(soft_boundary.get(i) != Some(&true));
                    subject_prefix.push(classes);
                }
                if track_shading {
                    shade += u16::from(is_subject && shaded[i]);
                    shade_prefix.push(shade);
                }
            }
        }
        Self {
            rgb,
            source_rgb,
            local,
            detail,
            weights,
            prefix,
            subject,
            soft_boundary,
            subject_prefix,
            shade_prefix,
        }
    }

    /// Seeds the base fill from the dominant RGB band and optionally pools similar colors.
    fn background(&self) -> u16 {
        let mut histogram = [0u16; 4096];
        for &[r, g, b] in &self.rgb {
            histogram
                [(usize::from(r >> 4) << 8) | (usize::from(g >> 4) << 4) | usize::from(b >> 4)] +=
                1;
        }
        let bin =
            histogram.iter().enumerate().max_by_key(|(_, n)| **n).expect("nonempty histogram").0;
        let seed = std::array::from_fn(|c| (((bin >> (8 - 4 * c)) & 15) * 16 + 8) as u8);
        let mut sums = [0u32; 3];
        let mut count = 0;
        for &rgb in &self.rgb {
            if crate::palette::distance(seed, rgb) <= regions::SIMILAR_COLOR_ERROR {
                for (sum, value) in sums.iter_mut().zip(rgb) {
                    *sum += u32::from(value);
                }
                count += 1;
            }
        }
        pack(sums.map(|sum| ((sum + count / 2) / count.max(1)) as u8))
    }
}

/// Rasterizes only the simplified semantic outlines that fit in the actual layer budget.
fn subject_mask(outlines: &[Outline]) -> Vec<bool> {
    if outlines.is_empty() {
        return Vec::new();
    }
    let mut mask = vec![false; W * H];
    for outline in outlines {
        let edges = Edges::new(outline, PLANE);
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| mask[y * W + l..y * W + r].fill(true));
        }
    }
    mask
}

/// Weighted fit score with the input-only squared norm removed.
fn fit(rgb: [u8; 3], sums: [i32; 4]) -> i64 {
    (0..3)
        .map(|c| {
            let v = i64::from(rgb[c]);
            v * (2 * i64::from(sums[c]) - i64::from(sums[3]) * v)
        })
        .sum()
}

/// Luminance used for both source and reconstructed feature contrast.
pub(super) fn luma([r, g, b]: [u8; 3]) -> i32 {
    (i32::from(r) + 2 * i32::from(g) + i32::from(b)) / 4
}

/// Chooses how a layer trades color error against the size and contrast of a feature.
#[derive(Clone, Copy)]
enum Priority {
    Color,
    Shape,
    Shade,
    Detail,
}

/// A color proposal plus coverage at the analysis resolution.
struct Candidate {
    patch: Patch,
    spans: Vec<(usize, usize, usize)>,
    score: i64,
    mass: i32,
    area: usize,
    detail: i32,
    crosses_subject: bool,
    shaded: bool,
}

impl Candidate {
    /// Fits this outline to the weighted mean color of its enclosed pixels.
    fn new(outline: Outline, target: &Target) -> Self {
        let edges = Edges::new(&outline, PLANE);
        let mut spans = Vec::new();
        let mut sums = [0i32; 4];
        let mut pixels = 0;
        let mut classes = [0usize; 2];
        let mut detail = 0;
        let mut shade = 0usize;
        for y in edges.top..edges.bottom {
            edges.spans(y, |l, r| {
                spans.push((y, l, r));
                let row = y * (W + 1);
                pixels += r - l;
                if !target.subject_prefix.is_empty() {
                    let left = target.subject_prefix[row + l];
                    let right = target.subject_prefix[row + r];
                    for c in 0..2 {
                        classes[c] += usize::from(right[c] - left[c]);
                    }
                }
                if !target.shade_prefix.is_empty() {
                    shade +=
                        usize::from(target.shade_prefix[row + r] - target.shade_prefix[row + l]);
                }
                if let Some(signal) = &target.detail {
                    detail += signal.span(y, l, r);
                }
                for (c, s) in sums.iter_mut().enumerate() {
                    *s += target.prefix[row + r][c] - target.prefix[row + l][c];
                }
            });
        }
        let color = pack(std::array::from_fn(|c| (sums[c] / sums[3].max(1)) as u8));
        Self {
            patch: Patch::polygon(outline, color),
            spans,
            score: fit(unpack(color), sums),
            mass: sums[3],
            area: pixels,
            detail: detail / pixels.max(1) as i32,
            crosses_subject: classes[0].min(classes[1]) > 4 + pixels / 64,
            shaded: pixels >= 900 && shade * 3 >= pixels * 2,
        }
    }

    /// Scores the error saved over existing fills after accounting for minor color variation.
    fn gain(&self, painting: &Painting, priority: Priority) -> i64 {
        if self.crosses_subject {
            return 0;
        }
        let minimum = match priority {
            Priority::Shade => 3 * 9 * 9,
            Priority::Shape if self.detail < 7 => 3 * 16 * 16,
            Priority::Detail if self.area > W * H / 22 || self.detail < 6 => return 0,
            Priority::Detail => 3 * 7 * 7,
            _ => regions::SIMILAR_COLOR_ERROR,
        };
        let floor = i64::from(self.mass) * i64::from(minimum);
        let saved = self.score
            - floor
            - self
                .spans
                .iter()
                .map(|&(y, l, r)| {
                    let row = y * (W + 1);
                    i64::from(painting.prefix[row + r] - painting.prefix[row + l])
                })
                .sum::<i64>();
        match priority {
            Priority::Detail => {
                saved * i64::from(self.detail.clamp(6, 40)) * 4 / (self.area + 24) as i64
            }
            _ => saved,
        }
    }

    /// Measures how repainting the polygon would change contrast across its visible boundary.
    fn boundary_gain(&self, source: &[[u8; 3]], painting: &Painting) -> i64 {
        let fill = unpack(self.patch.color);
        let gain = |inside: usize, outside: usize| {
            let mut score = 0;
            for (c, value) in fill.into_iter().enumerate() {
                let expected = i32::from(source[inside][c]) - i32::from(source[outside][c])
                    + i32::from(painting.colors[outside][c]);
                score += (expected - i32::from(painting.colors[inside][c])).pow(2)
                    - (expected - i32::from(value)).pow(2);
            }
            i64::from(score)
        };
        let transposed = self.patch.outline.map(|[x, y]| [y, x]);
        let scans = [
            (&self.patch.outline, PLANE, false),
            (&transposed, Canvas { width: H, height: W }, true),
        ];
        let mut score = 0;
        for (outline, grid, transposed) in scans {
            let index = |x, y| if transposed { x * W + y } else { y * W + x };
            let edges = Edges::new(outline, grid);
            for y in (edges.top..edges.bottom).step_by(2) {
                edges.spans(y, |left, right| {
                    if left > 0 {
                        score += gain(index(left, y), index(left - 1, y));
                    }
                    if right < grid.width {
                        score += gain(index(right - 1, y), index(right, y));
                    }
                });
            }
        }
        score
    }

    /// Nudges alternating vertices toward source boundaries while retaining the flat fill.
    fn refine(
        mut self,
        target: &Target,
        painting: &Painting,
        priority: Priority,
        phase: usize,
    ) -> Self {
        let source = target.source_rgb.as_deref().unwrap_or(&target.rgb);
        let rank = |candidate: &Self| {
            candidate.gain(painting, priority) + 8 * candidate.boundary_gain(source, painting)
        };
        let mut best = rank(&self);
        for offset in (0..VERTICES).step_by(2) {
            let i = (phase + offset) % VERTICES;
            let points = self.patch.outline;
            if points[..i].contains(&points[i]) {
                continue;
            }
            for (axis, shift) in [(0, -2), (0, 2), (1, -2), (1, 2)] {
                let mut vertex = points[i];
                vertex[axis] = (i16::from(vertex[axis]) + shift).clamp(0, 255) as u8;
                let outline = points.map(|point| if point == points[i] { vertex } else { point });
                let proposal = Self::new(outline, target);
                if proposal.area == 0 {
                    continue;
                }
                let score = rank(&proposal);
                if score > best + 2_000 + best.max(0) / 64 {
                    best = score;
                    self = proposal;
                }
            }
        }
        self
    }
}

/// Per-pixel and per-row running scores for the partially painted picture.
struct Painting {
    pixels: Vec<i32>,
    prefix: Vec<i32>,
    colors: Vec<[u8; 3]>,
}

impl Painting {
    /// Initializes the canvas and its score prefix sums with the background color.
    fn new(target: &Target, color: u16, track_colors: bool) -> Self {
        let mut painting = Self {
            pixels: vec![0; W * H],
            prefix: vec![0; (W + 1) * H],
            colors: if track_colors { vec![[0; 3]; W * H] } else { Vec::new() },
        };
        let rgb = unpack(color);
        for y in 0..H {
            painting.span(target, rgb, y, 0, W);
            painting.update(y);
        }
        painting
    }

    /// Sets scores for the pixels overwritten by one span.
    fn span(&mut self, target: &Target, color: [u8; 3], y: usize, l: usize, r: usize) {
        let range = y * W + l..y * W + r;
        if let Some(painted) = self.colors.get_mut(range.clone()) {
            painted.fill(color);
        }
        for i in range {
            let src = target.rgb[i];
            let score: i32 = (0..3)
                .map(|c| i32::from(color[c]) * (2 * i32::from(src[c]) - i32::from(color[c])))
                .sum();
            self.pixels[i] = score * target.weights[i];
        }
    }

    /// Rebuilds one row after it has been repainted.
    fn update(&mut self, y: usize) {
        let mut sum = 0;
        for (x, &score) in self.pixels[y * W..(y + 1) * W].iter().enumerate() {
            sum += score;
            self.prefix[y * (W + 1) + x + 1] = sum;
        }
    }

    /// Commits a polygon and refreshes just the rows crossed by that polygon.
    fn paint(&mut self, target: &Target, candidate: &Candidate) {
        let color = unpack(candidate.patch.color);
        let mut rows = [false; H];
        for &(y, l, r) in &candidate.spans {
            self.span(target, color, y, l, r);
            rows[y] = true;
        }
        for (y, changed) in rows.into_iter().enumerate() {
            if changed {
                self.update(y);
            }
        }
    }
}

/// Drops layers whose visible pixels can use the fills already underneath them.
fn merge_covered_colors(
    scene: &mut Scene,
    owners: &[u16],
    outline_count: usize,
    layout: &faces::Layout<'_>,
) -> bool {
    let mut underneath = vec![unpack(scene.background); W * H];
    let mut merged = false;
    for (i, patch) in scene.patches.iter_mut().enumerate() {
        let edges = Edges::new(&patch.outline, PLANE);
        let color = unpack(patch.color);
        let mut distinct = i < outline_count || layout.outline(i - outline_count).is_some();
        let mut visible = false;
        let slot = (i + 1) as u16;
        for y in edges.top..edges.bottom {
            if distinct {
                break;
            }
            edges.spans(y, |left, right| {
                let start = y * W + left;
                let end = y * W + right;
                distinct |= owners[start..end].iter().zip(&underneath[start..end]).any(
                    |(&owner, &base)| {
                        if owner != slot {
                            return false;
                        }
                        visible = true;
                        crate::palette::distance(base, color) > regions::SIMILAR_COLOR_ERROR
                    },
                );
            });
        }
        // Hidden outlines can reappear as the foreground moves.
        if distinct || !visible {
            for y in edges.top..edges.bottom {
                edges.spans(y, |left, right| underneath[y * W + left..y * W + right].fill(color));
            }
        } else if patch.outline != EMPTY_OUTLINE {
            // Keep landmark slots and previous-frame indices stable through the next recolor.
            patch.outline = EMPTY_OUTLINE;
            merged = true;
        }
    }
    merged
}

/// Squared RGB distance between two quantized fills.
fn distance(a: u16, b: u16) -> u32 {
    unpack(a).into_iter().zip(unpack(b)).map(|(u, v)| u32::from(u.abs_diff(v)).pow(2)).sum()
}

/// Reserves a little more fine detail when background texture dominates a guided scene.
#[derive(Default)]
struct FineReserve {
    support: u8,
    expanded: bool,
}

impl FineReserve {
    const EXTRA_SLOTS: usize = 4;

    /// Compares edge density inside and outside the subject and changes the reserve gradually.
    fn update(&mut self, subject: &[bool], edges: &[u8], strong_edges: usize) -> usize {
        let (mut area, mut strong) = (0, 0);
        for (&inside, &edge) in subject.iter().zip(edges) {
            if inside {
                area += 1;
                strong += usize::from(edge >= 24);
            }
        }
        // Cross-multiply to compare edge densities without dividing by the region areas.
        let foreground = (strong * (W * H - area)) as u64;
        let background = ((strong_edges - strong) * area) as u64;
        let ratio = if self.expanded { 16 } else { 13 };
        if (W * H / 8..W * H * 3 / 4).contains(&area) && foreground * 25 < background * ratio {
            self.support = (self.support + 2).min(12);
        } else {
            self.support = self.support.saturating_sub(3);
        }
        self.expanded = self.support >= if self.expanded { 4 } else { 8 };
        Self::EXTRA_SLOTS * usize::from(self.expanded)
    }
}

/// Retains geometry and visible colors across frames until the current source favors an update.
#[derive(Default)]
pub(crate) struct Tracker {
    scene: Scene,
    last: Vec<[u8; 3]>,
    last_detail: Vec<[u8; 3]>,
    last_limit: u16,
    fine_start: usize,
    tick: u32,
    features: bool,
    budgeted_search: bool,
    line_strokes: bool,
    textured: bool,
    faces: faces::Faces,
    #[cfg(feature = "object-layers")]
    objects: objects::Objects,
    #[cfg(feature = "object-layers")]
    parts: Option<parts::Parts>,
    previous_guides: usize,
    previous_subjects: usize,
    fine_reserve: FineReserve,
}

impl Tracker {
    /// Configures optional sender-side models and temporal retention for the layered mode.
    pub(crate) fn new(guidance: Guidance) -> anyhow::Result<Self> {
        Ok(Self {
            features: guidance.features,
            budgeted_search: guidance.budgeted_search,
            line_strokes: guidance.line_strokes,
            faces: faces::Faces::new(guidance.faces)?,
            #[cfg(feature = "object-layers")]
            objects: objects::Objects::new(
                guidance.objects,
                guidance.object_provider,
                guidance.object_model.as_deref(),
                guidance.features,
            )?,
            #[cfg(feature = "object-layers")]
            parts: guidance.parts.map(|(path, mode)| parts::Parts::new(&path, mode)).transpose()?,
            ..Self::default()
        })
    }

    /// Reports whether this encoder writes typed line strokes in layered frames.
    pub(crate) fn uses_strokes(&self) -> bool {
        self.line_strokes
    }

    /// Fits the best translated version of an existing outline with a bias toward keeping its position.
    fn follow(
        &self,
        old: &Patch,
        target: &Target,
        painting: &Painting,
        priority: Priority,
    ) -> Candidate {
        let mut best = Candidate::new(old.outline, target);
        let mut score = best.gain(painting, priority);
        let move_cost = 32;
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-2, -2), (2, -2), (-2, 2), (2, 2)] {
            let outline = old.outline.map(|[x, y]| {
                [
                    (i32::from(x) + dx).clamp(0, 255) as u8,
                    (i32::from(y) + dy * 2).clamp(0, 255) as u8,
                ]
            });
            let other = Candidate::new(outline, target);
            let gain = other.gain(painting, priority) - i64::from(other.mass) * move_cost;
            if gain > score {
                score = gain;
                best = other;
            }
        }
        best
    }

    /// Refits each fill using only its exposed pixels after foreground polygons occlude it.
    fn recolor(
        &self,
        scene: &mut Scene,
        target: &Target,
        previous: Option<&Scene>,
        owners: &[u16],
    ) {
        let mut sums = vec![[0i32; 4]; scene.len() + 1];
        for (i, &owner) in owners.iter().enumerate() {
            let s = &mut sums[usize::from(owner)];
            let w = target.weights[i];
            for (c, &v) in target.rgb[i].iter().enumerate() {
                s[c] += i32::from(v) * w;
            }
            s[3] += w;
        }
        for (i, (dst, s)) in std::iter::once(&mut scene.background)
            .chain(scene.patches.iter_mut().map(|p| &mut p.color))
            .zip(sums)
            .enumerate()
        {
            if s[3] == 0 {
                continue;
            }
            let color = pack(std::array::from_fn(|c| (s[c] / s[3]) as u8));
            let old = previous.and_then(|p| {
                if i == 0 {
                    Some(p.background)
                } else {
                    p.patches.get(i - 1).map(|p| p.color)
                }
            });
            *dst = old.filter(|&v| distance(v, color) <= 100).unwrap_or(color);
        }
    }

    /// Builds one self-contained snapshot; prediction on the wire is handled by the packet codec.
    pub(crate) fn encode(&mut self, input: &[u8], size: Canvas, limit: u16, out: &mut Vec<u8>) {
        let pixels = input.as_chunks::<3>().0;
        let sample = |x, y| pixels[y * size.width + x];
        let rgb = if size == PLANE { pixels.to_vec() } else { sample_plane(size, sample) };
        let detail = if self.features
            && size.width >= DETAIL_PLANE.width
            && size.height >= DETAIL_PLANE.height
        {
            if size == DETAIL_PLANE {
                std::borrow::Cow::Borrowed(pixels)
            } else {
                std::borrow::Cow::Owned(sample_grid(size, DETAIL_PLANE, sample))
            }
        } else {
            std::borrow::Cow::Borrowed(&[][..])
        };
        let old = self.last.as_slice();
        let changed = rgb != old;
        let cut = old.is_empty()
            || (changed
                && rgb
                    .iter()
                    .zip(old)
                    .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| usize::from(a.abs_diff(*b))))
                    .sum::<usize>()
                    >= W * H * 3 * 28);
        let fresh_guides = self.faces.update(input, size, old, &rgb, cut, changed);
        #[cfg(feature = "object-layers")]
        let (fresh_subjects, selected) = {
            let fresh = self.objects.update(old, &rgb, &detail, cut, changed);
            (fresh, self.objects.outlines(usize::from(limit), self.features))
        };
        #[cfg(feature = "object-layers")]
        let outlines = selected.as_slice();
        #[cfg(not(feature = "object-layers"))]
        let (fresh_subjects, outlines): (bool, &[Outline]) = (false, &[]);
        let subjects = outlines.len();
        #[cfg(feature = "object-layers")]
        let new_parts = self.parts.as_mut().is_some_and(|parts| {
            parts.advance(cut, changed || detail.as_ref() != self.last_detail)
        });
        #[cfg(not(feature = "object-layers"))]
        let new_parts = false;
        let same_config = self.last_limit == limit;
        if same_config
            && !fresh_guides
            && !fresh_subjects
            && !new_parts
            && !cut
            && !changed
            && detail.as_ref() == self.last_detail
        {
            self.scene.write(out, self.line_strokes);
            return;
        }
        let remaining = usize::from(limit) - subjects;
        let guides = &self.faces.guides[..self.faces.guides.len().min(remaining)];
        let previous = (!cut
            && self.previous_guides == guides.len()
            && self.previous_subjects == subjects
            && same_config)
            .then_some(&self.scene);
        self.previous_guides = guides.len();
        self.previous_subjects = subjects;
        self.last_limit = limit;
        self.tick = self.tick.wrapping_add(1);
        #[cfg(feature = "object-layers")]
        let focus = self.parts.as_ref().map_or(&[][..], |parts| parts.focus());
        #[cfg(not(feature = "object-layers"))]
        let focus = &[];
        // Hysteresis prevents a small change in edge density from repeatedly switching the fit.
        let texture_threshold = W * H / if self.textured && !cut { 6 } else { 5 };
        let budgeted_detail = (self.budgeted_search && self.features)
            .then(|| detail::Detail::budgeted(&rgb, &detail));
        let (working_rgb, original) = if let Some(mark) = &budgeted_detail {
            if mark.samples.is_empty() {
                (rgb, None)
            } else {
                let restored = rgb
                    .iter()
                    .zip(&mark.samples)
                    .map(|(&color, &sample)| sample.unwrap_or(color))
                    .collect();
                (restored, Some(rgb))
            }
        } else {
            (rgb, None)
        };
        let mut target = Target::new(
            working_rgb,
            guides,
            outlines,
            (self.features && !self.budgeted_search).then_some(detail.as_ref()),
            focus,
            texture_threshold,
        );
        if let Some(mark) = budgeted_detail {
            target.detail = Some(mark);
            target.source_rgb = original;
        }
        self.textured = !self.budgeted_search
            && target
                .detail
                .as_ref()
                .is_some_and(|detail| detail.strong_edges >= texture_threshold);
        let background = previous.map_or_else(|| target.background(), |p| p.background);
        let mut painting = Painting::new(&target, background, self.textured);
        let proposals = if self.budgeted_search {
            regions::budgeted_proposals(
                &target.rgb,
                &target.subject,
                target.detail.as_ref().map(|detail| detail.fine.as_slice()),
            )
        } else {
            regions::proposals(
                &target.rgb,
                target.source_rgb.as_deref().unwrap_or(&target.rgb),
                &target.local,
                &target.subject,
                &target.soft_boundary,
                target.detail.as_ref(),
            )
        };
        let mut candidates: Vec<_> = proposals
            .into_iter()
            .map(|s| Candidate::new(s, &target))
            .filter(|candidate| !candidate.crosses_subject)
            .collect();
        let mut scene = Scene { background, patches: Vec::new() };
        let fine_slots = if !self.budgeted_search && guides.is_empty() && !detail.is_empty() {
            remaining / 5
        } else {
            0
        };
        const MIN_FINE_SLOTS: usize = 4;
        let prefer_coarse_marks = fine_slots > MIN_FINE_SLOTS
            && !self.textured
            && target
                .detail
                .as_ref()
                .is_some_and(|detail| detail.supports_coarse_marks(&target.subject));
        let fine_slots = if prefer_coarse_marks { MIN_FINE_SLOTS } else { fine_slots };
        let can_expand = self.textured && fine_slots >= 6 && !target.subject.is_empty();
        if cut || !same_config || !can_expand {
            self.fine_reserve = FineReserve::default();
        }
        let extra = if can_expand {
            target.detail.as_ref().map_or(0, |detail| {
                self.fine_reserve.update(&target.subject, &detail.edges, detail.strong_edges)
            })
        } else {
            0
        };
        let fine_slots = fine_slots + extra;
        let broad_slots = remaining - fine_slots;
        let flexible_budget = fine_slots != 0 && !self.textured && !prefer_coarse_marks;
        let optional_fill_start = subjects + remaining - remaining / 3;
        let layout = faces::Layout::new(guides, broad_slots);
        let object_slots = outlines.iter().copied().map(Some);
        let face_slots = (0..broad_slots).map(|i| layout.outline(i));
        let detail_start = if fine_slots == 0 || prefer_coarse_marks {
            usize::from(limit) - remaining.div_ceil(4)
        } else {
            subjects + broad_slots
        };
        let mut detail_phase = false;
        for (i, guided) in object_slots.chain(face_slots).enumerate() {
            if let Some(outline) = guided {
                let candidate = Candidate::new(outline, &target);
                painting.paint(&target, &candidate);
                scene.patches.push(candidate.patch);
                continue;
            }
            let priority = if !self.features {
                Priority::Color
            } else if detail_phase || i >= detail_start {
                Priority::Detail
            } else if self.budgeted_search {
                Priority::Color
            } else {
                Priority::Shape
            };
            let mut select = |priority| {
                let gain = |c: &Candidate| c.gain(&painting, priority);
                // Give a coherent light or shadow fill one early slot on the foreground.
                if i == subjects && matches!(priority, Priority::Shape) {
                    let shade = candidates
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.shaded && c.detail <= 4)
                        .map(|(j, c)| (j, c.gain(&painting, Priority::Shade)))
                        .filter(|&(_, gain)| gain > 900_000)
                        .max_by_key(|&(_, gain)| gain);
                    if let Some((j, _)) = shade {
                        return Some(candidates.swap_remove(j));
                    }
                }
                let tracked = previous
                    .filter(|_| i < self.fine_start)
                    .and_then(|p| p.patches.get(i))
                    .map(|p| {
                        if self.budgeted_search {
                            Candidate::new(p.outline, &target)
                        } else {
                            self.follow(p, &target, &painting, priority)
                        }
                    })
                    .filter(|c| gain(c) > 300);
                let best =
                    if tracked.is_none() || (i as u32).wrapping_add(self.tick).is_multiple_of(8) {
                        candidates.iter().map(gain).enumerate().max_by_key(|&(_, g)| g)
                    } else {
                        None
                    };
                match (tracked, best) {
                    (Some(old), Some((j, g))) if g > gain(&old) * 3 / 2 + 1500 => {
                        Some(candidates.swap_remove(j))
                    }
                    (Some(old), _) => Some(old),
                    (None, Some((j, g))) if g > 300 => Some(candidates.swap_remove(j)),
                    _ => None,
                }
            };
            let choice = select(priority).or_else(|| {
                if matches!(priority, Priority::Shape)
                    || (self.budgeted_search
                        && self.features
                        && matches!(priority, Priority::Color))
                {
                    detail_phase = true;
                    select(Priority::Detail)
                } else if self.budgeted_search && matches!(priority, Priority::Detail) {
                    select(Priority::Color)
                } else if prefer_coarse_marks && matches!(priority, Priority::Detail) {
                    select(Priority::Shape)
                } else {
                    None
                }
            });
            let mut choice = match choice {
                Some(choice) => choice,
                None if layout.needs_padding(i - subjects) => {
                    // Preserve the z positions of face guides even on a completely flat background.
                    Candidate::new(EMPTY_OUTLINE, &target)
                }
                None => break,
            };
            if flexible_budget
                && i >= optional_fill_start
                && choice.gain(&painting, priority) < OPTIONAL_FILL_GAIN
            {
                break;
            }
            // Rotate through a quarter of the layers per frame to bound the extra fitting work.
            if self.textured && i % 4 == self.tick as usize % 4 {
                choice = choice.refine(&target, &painting, priority, self.tick as usize);
            }
            painting.paint(&target, &choice);
            scene.patches.push(choice.patch);
        }
        let fine_slots = if flexible_budget || prefer_coarse_marks {
            (remaining / 3).min(usize::from(limit) - scene.len())
        } else {
            fine_slots
        };
        let owners = scene.owners(PLANE);
        self.recolor(&mut scene, &target, previous, &owners);
        if !scene.patches.is_empty() && merge_covered_colors(&mut scene, &owners, subjects, &layout)
        {
            let owners = scene.owners(PLANE);
            self.recolor(&mut scene, &target, previous, &owners);
            if guides.is_empty() {
                scene.patches.retain(|p| p.outline != EMPTY_OUTLINE);
            }
        }
        // Facial landmarks can be smaller than one analysis cell. Sample their transmitted outlines at source resolution.
        for (i, &(part, outline)) in layout.features.iter().enumerate() {
            let slot = subjects + layout.detail_start + i;
            let patch = &mut scene.patches[slot];
            let edges = Edges::new(&outline, size);
            if let Some(color) = faces::feature_color(&edges, part, input, size) {
                patch.color = previous
                    .and_then(|p| p.patches.get(slot))
                    .map(|p| p.color)
                    .filter(|&old| distance(old, color) <= 60)
                    .unwrap_or(color);
            }
        }
        let fine_start = scene.len();
        if let Some(features) = &target.detail {
            if fine_slots > 0 {
                // Fine candidates are rechecked against the source even when subject outlines change.
                let previous_fine =
                    if !cut && same_config { &self.scene.patches[self.fine_start..] } else { &[] };
                fine::append(
                    &mut scene,
                    fine::Input {
                        rgb: &detail,
                        signal: &features.dense_signal,
                        subject: &target.subject,
                        previous: previous_fine,
                        previous_rgb: &self.last_detail,
                        slots: fine_slots,
                        textured: self.textured,
                    },
                    #[cfg(feature = "object-layers")]
                    self.parts.as_mut(),
                );
            }
        }
        if self.line_strokes {
            for (i, patch) in scene.patches.iter_mut().enumerate().skip(subjects) {
                if layout.outline(i - subjects).is_none() {
                    patch.stroke = previous
                        .and_then(|scene| scene.patches.get(i))
                        .filter(|old| old.outline == patch.outline)
                        .and_then(|old| old.stroke)
                        .or_else(|| Stroke::fit(&patch.outline));
                }
            }
        }
        scene.write(out, self.line_strokes);
        self.scene = scene;
        self.fine_start = fine_start;
        self.last = target.source_rgb.unwrap_or(target.rgb);
        self.last_detail.clear();
        self.last_detail.extend_from_slice(&detail);
    }
}

#[cfg(test)]
mod feature_tests {
    use super::*;

    /// Expands the reserve only after a calmer foreground persists over several frames.
    #[test]
    fn detail_reserve_ignores_a_single_texture_change() {
        let area = W * H / 4;
        let subject: Vec<_> = (0..W * H).map(|i| i < area).collect();
        let mut edges = vec![32; W * H];
        edges[..area].fill(0);
        let mut reserve = FineReserve::default();
        for _ in 0..3 {
            assert_eq!(reserve.update(&subject, &edges, W * H - area), 0);
        }
        assert_eq!(reserve.update(&subject, &edges, W * H - area), 4);

        edges[..area].fill(32);
        assert_eq!(reserve.update(&subject, &edges, W * H), 4);
        assert_eq!(reserve.update(&subject, &edges, W * H), 0);
    }

    /// An outline can recover a shifted boundary using the source colors on both sides.
    #[test]
    fn refines_a_shifted_outline_against_the_source() {
        let background = [29, 39, 50];
        let mut rgb = vec![background; W * H];
        let correct = [
            [40, 40],
            [176, 40],
            [176, 168],
            [40, 168],
            [40, 168],
            [40, 168],
            [40, 168],
            [40, 168],
        ];
        let edges = Edges::new(&correct, PLANE);
        for y in edges.top..edges.bottom {
            edges.spans(y, |left, right| rgb[y * W + left..y * W + right].fill([218, 139, 62]));
        }
        let target = Target::new(rgb, &[], &[], None, &[], W * H / 5);
        let painting = Painting::new(&target, pack(background), true);
        let mut shifted = correct;
        shifted[0][0] -= 2;
        shifted[2][0] += 2;
        let initial = Candidate::new(shifted, &target);
        let before = initial.boundary_gain(&target.rgb, &painting);
        let refined = initial.refine(&target, &painting, Priority::Shape, 0);
        assert!(refined.boundary_gain(&target.rgb, &painting) > before);
    }

    /// Flat lighting tones leave budget for compact marks on the same connected surface.
    #[test]
    fn retains_marks_while_reducing_a_lighting_ramp() -> anyhow::Result<()> {
        let rgb: Vec<_> = (0..W * H)
            .map(|i| {
                let (x, y) = (i % W, i / W);
                if !(48..144).contains(&x) || !(14..96).contains(&y) {
                    return [20, 45, 78];
                }
                let shade = 104 + ((x - 48) * 5 / 6) as u8;
                let color = [shade + 18, shade, shade - 10];
                if (65..76).contains(&x) && (42..46).contains(&y)
                    || (110..122).contains(&x) && (42..46).contains(&y)
                {
                    color.map(|v| v - 53)
                } else if (88..110).contains(&x) && (69..74).contains(&y) {
                    [shade - 10, shade - 48, shade - 53]
                } else {
                    color
                }
            })
            .collect();
        let target = Target::new(rgb.clone(), &[], &[], Some(&[]), &[], W * H / 5);
        let ramp: std::collections::HashSet<_> =
            target.rgb[25 * W + 54..25 * W + 138].iter().copied().collect();
        assert_eq!(ramp.len(), 2);
        for budgeted_search in [false, true] {
            let mut tracker = Tracker { features: true, budgeted_search, ..Tracker::default() };
            let mut encoded = Vec::new();
            tracker.encode(rgb.as_flattened(), PLANE, 12, &mut encoded);
            let scene = Scene::read(&encoded, false)?;
            let frame = scene.render(PLANE);
            let green = |x, y| i32::from(frame[(y * W + x) * 3 + 1]);
            for (x, y) in [(70, 44), (115, 44), (100, 71)] {
                assert!(green(x, y - 7) - green(x, y) > 20, "missing mark at ({x},{y})");
            }
            assert!(green(120, 25) - green(75, 25) > 25);
            assert!(scene.len() <= 12);
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "object-layers"))]
mod object_tests {
    use super::*;

    /// Guided surfaces retain both lighting tones and their dark marks within 48 layers.
    #[test]
    fn keeps_a_coherent_shadow_under_moving_detail() -> anyhow::Result<()> {
        check_coherent_shadow(false)
    }

    /// A textured background leaves room for both lighting tones on a smooth subject.
    #[test]
    fn keeps_a_coherent_shadow_against_a_textured_background() -> anyhow::Result<()> {
        check_coherent_shadow(true)
    }

    /// Measures the light and shadow fills and two small marks on a moving guided surface.
    fn check_coherent_shadow(textured: bool) -> anyhow::Result<()> {
        let mut tracker = Tracker { features: true, ..Tracker::default() };
        let mut encoded = Vec::new();
        for left in [79, 80, 82, 84] {
            let inside = |x, y| (left..left + 210).contains(&x) && (47..170).contains(&y);
            let mask: Vec<_> =
                (0..W * H).map(|i| inside(2 * (i % W) + 1, 2 * (i / W) + 1)).collect();
            tracker.objects = objects::Objects::with_silhouettes(&mask);
            let width = DETAIL_PLANE.width;
            let rgb: Vec<_> = (0..width * DETAIL_PLANE.height)
                .map(|i| {
                    let (x, y) = (i % width, i / width);
                    if !inside(x, y) {
                        return if textured && (x / 6 + y / 8) % 2 == 0 {
                            [168, 120, 95]
                        } else {
                            [40, 49, 60]
                        };
                    }
                    if ((left + 60..left + 81).contains(&x) && (89..98).contains(&y))
                        || ((left + 118..left + 144).contains(&x) && (123..132).contains(&y))
                    {
                        return [49, 52, 62];
                    }
                    let light = ((x - left) * 44 / 210) as u8;
                    [106 + light, 116 + light, 129 + light]
                })
                .collect();
            tracker.encode(rgb.into_flattened().as_slice(), DETAIL_PLANE, 48, &mut encoded);
            assert_eq!(tracker.textured, textured);
            let scene = Scene::read(&encoded, false)?;
            let image = scene.render(DETAIL_PLANE);
            let green = |x, y| i32::from(image[3 * (y * width + x) + 1]);
            assert!(green(left + 155, 70) - green(left + 40, 70) >= 15);
            assert!(green(left + 70, 82) - green(left + 70, 93) >= 40);
            assert!(green(left + 130, 115) - green(left + 130, 127) >= 40);
            assert!(scene.len() <= 48);
            encoded.clear();
        }
        Ok(())
    }

    /// Extra contours preserve gaps between thin protrusions under the same wire format.
    #[test]
    fn partitions_a_complex_silhouette() {
        let mask: Vec<_> = (0..W * H)
            .map(|i| {
                let (x, y) = (i % W, i / W);
                ((35..153).contains(&x) && (22..61).contains(&y))
                    || ((60..92).contains(&y)
                        && ((41..54).contains(&x)
                            || (81..94).contains(&x)
                            || (129..142).contains(&x)))
            })
            .collect();
        let guides = regions::silhouettes(&mask, false);
        assert_eq!(guides.len(), 1);
        let guide = &guides[0];
        assert!((2..=4).contains(&guide.parts.len()));
        let error = |outlines: &[Outline]| {
            subject_mask(outlines).iter().zip(&mask).filter(|(a, b)| a != b).count()
        };
        assert!(error(&guide.parts) < error(&[guide.outline]));
        assert!(regions::silhouettes(&vec![true; W * H], true).is_empty());
    }

    /// Coverage scoring retains the narrow, angled parts of a partitioned subject.
    #[test]
    fn improves_partitioned_mask_coverage() {
        let mask: Vec<_> = (0..W * H)
            .map(|i| {
                let (x, y) = (i % W, i / W);
                let body = (35..153).contains(&x) && (22..61).contains(&y);
                let side = (153..168).contains(&x) && (40..54).contains(&y);
                let limbs = (59..92).contains(&y)
                    && (0..4).any(|k| {
                        let bend = (y as isize - 60).div_euclid(2 + k % 2);
                        let left = (41 + k * 34 + bend) as usize;
                        (left..left + 7).contains(&x)
                    });
                body || side || limbs
            })
            .collect();
        let ordinary = regions::silhouettes(&mask, false);
        let refined = regions::silhouettes(&mask, true);
        let error = |parts: &[Outline]| {
            subject_mask(parts).iter().zip(&mask).filter(|(a, b)| a != b).count()
        };
        assert!((2..=4).contains(&ordinary[0].parts.len()));
        assert!((2..=4).contains(&refined[0].parts.len()));
        assert!(error(&refined[0].parts) < error(&ordinary[0].parts));
    }
}
