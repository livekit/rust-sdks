//! Online flat-color vectorization using a temporally regularized triangular mesh.
mod flat;
mod importance;
mod patch;
mod predict;
pub(crate) use predict::MeshCoding;

use anyhow::{ensure, Context, Result};
use importance::ImportanceMap;

use crate::{
    palette,
    video::{Canvas, HEIGHT, WIDTH},
};

const SCALE: usize = 2;

/// Dimensions of the source raster after taking one sample per 2×2 block.
pub(crate) fn sample_canvas(canvas: Canvas) -> Canvas {
    Canvas { width: canvas.width / SCALE, height: canvas.height / SCALE }
}

/// A solid triangle with positions normalized to the video rectangle.
#[derive(PartialEq)]
pub(crate) struct DisplayTriangle {
    pub(crate) points: [[f32; 2]; 3],
    pub(crate) rgb: [u8; 3],
}

// Ordered by wire cost. Both endpoints reconstruct the same triangle connectivity.
const GRIDS: &[(usize, usize)] =
    &[(1, 1), (2, 1), (3, 2), (5, 3), (7, 4), (9, 5), (10, 5), (12, 7), (14, 8), (16, 9), (18, 10)];
/// Matches the previous 720p mesh of 168 base faces and up to 88 subdivisions.
pub(crate) const DEFAULT_TRIANGLES: u32 = 432;

type Offset = [i8; 2];
type Point = [i32; 2];
type Face = [usize; 3];

/// Per-sample squared RGB error allowed when retaining a previous palette assignment.
const COLOR_HYSTERESIS: i64 = 288;
/// Accepts a larger color residual to retain a transmitted palette index.
const LOW_BANDWIDTH_COLOR_HYSTERESIS: i64 = 5600;
/// Reuses the mesh structure across short runs while updating colors every frame.
const STRUCTURE_PERIOD: u8 = 6;
/// Large image changes interrupt a run of reused structure.
const STRUCTURE_REFRESH_DELTA: u32 = 30;

/// Selects which image regions receive more triangles.
#[derive(Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum DetailPriority {
    /// Fit colors and geometry to corners, lines and locally distinctive features.
    #[default]
    Features,
    /// Favor locally distinctive structure that survives spatial averaging.
    Structure,
    /// Minimize squared RGB error across all pixels equally.
    ColorError,
}

/// Configures sender-side triangle fitting and optional rate reduction.
#[derive(Default)]
pub(crate) struct Options {
    pub(crate) priority: DetailPriority,
    pub(crate) merge_flat_regions: bool,
    pub(crate) low_bandwidth: bool,
}

/// Controls color changes and structural reuse for one fitted picture.
#[derive(Clone, Copy)]
struct Update {
    hold_structure: bool,
    color_hysteresis: i64,
    palette_size: usize,
}

/// A regular grid with a shared diagonal pattern and movable vertices.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grid {
    Preset(u8),
    Sized { cols: u16, rows: u16 },
}

impl Grid {
    /// Returns the number of cells across and down the picture.
    fn dimensions(self) -> (usize, usize) {
        match self {
            Self::Preset(i) => GRIDS[usize::from(i)],
            Self::Sized { cols, rows } => (usize::from(cols), usize::from(rows)),
        }
    }

    /// Keeps subcell offsets distinct on grids finer than the legacy coordinate raster.
    fn precision(self) -> usize {
        match self {
            Self::Preset(_) => 1,
            Self::Sized { .. } => 64,
        }
    }

    /// Reads the legacy level or explicit grid dimensions before allocating any mesh arrays.
    pub(crate) fn read(data: &mut &[u8], canvas: Canvas) -> Result<Self> {
        let (&level, rest) = data.split_first().context("Missing polygon grid")?;
        *data = rest;
        if level == u8::MAX {
            let (dims, rest) =
                data.split_first_chunk::<4>().context("Missing polygon grid dimensions")?;
            let cols = u16::from_le_bytes([dims[0], dims[1]]);
            let rows = u16::from_le_bytes([dims[2], dims[3]]);
            ensure!(
                cols > 0
                    && usize::from(cols) <= canvas.width / SCALE
                    && rows > 0
                    && usize::from(rows) <= canvas.height / SCALE,
                "Polygon grid exceeds the analysis raster"
            );
            *data = rest;
            Ok(Self::Sized { cols, rows })
        } else {
            ensure!(usize::from(level) < GRIDS.len(), "Unknown polygon grid");
            Ok(Self::Preset(level))
        }
    }

    /// Writes one preset index or a marker and little-endian cell dimensions.
    pub(crate) fn write(self, out: &mut Vec<u8>) {
        match self {
            Self::Preset(level) => out.push(level),
            Self::Sized { cols, rows } => {
                out.push(u8::MAX);
                out.extend_from_slice(&cols.to_le_bytes());
                out.extend_from_slice(&rows.to_le_bytes());
            }
        }
    }

    /// Counts the encoded topology descriptor.
    fn descriptor_bytes(self) -> usize {
        match self {
            Self::Preset(_) => 1,
            Self::Sized { .. } => 5,
        }
    }

    /// Counts the two base triangles in each rectangular cell.
    fn face_count(self) -> usize {
        let (w, h) = self.dimensions();
        2 * w * h
    }

    /// Computes exactly how many bytes this mesh occupies after the packet header.
    fn bytes(self) -> usize {
        let (w, h) = self.dimensions();
        self.descriptor_bytes() + self.vertices() + (4 * w * h).min(32 + w * h)
    }

    /// Counts vertices including the four pinned canvas corners.
    fn vertices(self) -> usize {
        let (w, h) = self.dimensions();
        (w + 1) * (h + 1)
    }

    /// Reconstructs a vertex on the canvas from its signed offsets in sixteenths of a cell.
    fn point(self, i: usize, [dx, dy]: Offset) -> Point {
        let (w, h) = self.dimensions();
        let axis = |k: usize, d: i8, cells: usize, extent: usize| {
            (((16 * k as i64 + i64::from(d)) * (extent * self.precision()) as i64
                + 8 * cells as i64)
                / (16 * cells as i64)) as i32
        };
        [axis(i % (w + 1), dx, w, WIDTH), axis(i / (w + 1), dy, h, HEIGHT)]
    }

    /// Boundary vertices can slide along the frame edge only.
    fn allowed(self, i: usize, [dx, dy]: Offset) -> bool {
        let (w, h) = self.dimensions();
        let (x, y) = (i % (w + 1), i / (w + 1));
        (-7..=7).contains(&dx)
            && (-7..=7).contains(&dy)
            && (dx == 0 || (x > 0 && x < w))
            && (dy == 0 || (y > 0 && y < h))
    }

    /// Enumerates the same counterclockwise faces for the entire stream.
    fn faces(self) -> impl Iterator<Item = Face> {
        let (w, h) = self.dimensions();
        (0..w * h).flat_map(move |i| {
            let a = i + i / w;
            let (b, c, d) = (a + 1, a + w + 1, a + w + 2);
            if (i % w + i / w).is_multiple_of(2) {
                [[a, b, d], [a, d, c]]
            } else {
                [[a, b, c], [b, d, c]]
            }
        })
    }
}

/// The stable base grid and maximum number of four-way subdivisions for each picture.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct MeshPlan {
    grid: Grid,
    splits: usize,
}

impl MeshPlan {
    /// Reserves more independently movable vertices for fitting feature boundaries.
    pub(crate) fn for_triangles(
        limit: u32,
        canvas: Canvas,
        priority: DetailPriority,
        low_bandwidth: bool,
    ) -> Self {
        let limit = limit as usize;
        let fifths = if priority == DetailPriority::Features && !low_bandwidth { 3 } else { 2 };
        let base_target = (limit as u64 * fifths / 5) as usize;
        let grid = if base_target <= Grid::Preset((GRIDS.len() - 1) as u8).face_count() {
            (0..GRIDS.len())
                .map(|i| Grid::Preset(i as u8))
                .filter(|g| g.face_count() <= limit)
                .min_by_key(|g| {
                    let base = g.face_count();
                    let possible = if base * 2 <= 32 + base / 2 { base } else { 4 * base };
                    (limit.saturating_sub(possible), base.abs_diff(base_target))
                })
                .expect("the smallest grid has two triangles")
        } else {
            // Select near-square image cells; a grid finer than the input samples adds no detail.
            let (w, h) = (canvas.width / SCALE, canvas.height / SCALE);
            let cells = (base_target / 2).clamp(1, w * h);
            let cols = ((cells as f64 * w as f64 / h as f64).sqrt().round() as usize)
                .clamp(1, w.min(cells));
            let rows = cells.div_ceil(cols).min(h);
            // Prefer nearby dimensions that can spend the final one or two triangles.
            let mut best = (cols, rows);
            let mut score = (usize::MAX, usize::MAX);
            for x in cols.saturating_sub(2).max(1)..=(cols + 2).min(w) {
                for y in rows.saturating_sub(2).max(1)..=(rows + 2).min(h) {
                    let base = 2 * x * y;
                    if base > limit {
                        continue;
                    }
                    let attainable = base + 3 * base.min((limit - base) / 3);
                    let cost = (
                        limit - attainable,
                        base.abs_diff(2 * cells)
                            + x.abs_diff(cols) * rows
                            + y.abs_diff(rows) * cols,
                    );
                    if cost < score {
                        score = cost;
                        best = (x, y);
                    }
                }
            }
            Grid::Sized { cols: best.0 as u16, rows: best.1 as u16 }
        };
        let base = grid.face_count();
        let splits = if base * 2 <= 32 + base / 2 { 0 } else { base.min((limit - base) / 3) };
        Self { grid, splits }
    }

    /// Counts the most triangles this layout can render, including child faces.
    pub(crate) fn max_triangles(self) -> usize {
        self.grid.face_count() + 3 * self.splits
    }

    /// Counts the uncompressed bytes with every permitted subdivision present.
    pub(crate) fn body_bytes(self) -> usize {
        let detail = if self.splits == 0 {
            0
        } else {
            self.grid.face_count().div_ceil(8) + (3 * self.splits).div_ceil(2)
        };
        self.grid.bytes() + detail
    }
}

/// A self-contained set of shared vertices and one flat RGB565 color per face.
pub(crate) struct Mesh {
    pub(crate) grid: Grid,
    offsets: Vec<Offset>,
    colors: FaceColors,
    detail: Vec<Option<[u8; 4]>>,
}

impl Mesh {
    /// Serializes geometry in stable vertex order followed by the face colors.
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        self.write_fills(&self.colors, &self.detail, out);
    }

    /// Writes the shared geometry with the supplied face colors and subdivisions.
    fn write_fills(&self, colors: &FaceColors, detail: &[Option<[u8; 4]>], out: &mut Vec<u8>) {
        self.grid.write(out);
        out.extend(self.offsets.iter().map(|&[x, y]| ((x + 7) as u8) << 4 | (y + 7) as u8));
        colors.write(out, detail);
        if !detail.is_empty() {
            for group in detail.chunks(8) {
                out.push(
                    group
                        .iter()
                        .enumerate()
                        .fold(0, |mask, (i, entry)| mask | (u8::from(entry.is_some()) << i)),
                );
            }
            // The base color slot already carries the center child's index.
            let corners = detail.iter().flatten().flat_map(|indices| indices[..3].iter().copied());
            write_indices(out, corners);
        }
    }

    /// Validates the fixed packet length, border constraints and all triangle windings.
    pub(crate) fn decode(mut data: &[u8], canvas: Canvas) -> Result<Self> {
        let grid = Grid::read(&mut data, canvas)?;
        let body_bytes = grid.bytes() - grid.descriptor_bytes();
        let (body, tail) =
            data.split_at_checked(body_bytes).context("Truncated polygon payload")?;
        let (geometry, colors) = body.split_at(grid.vertices());
        let mut offsets = Vec::with_capacity(geometry.len());
        for (i, &byte) in geometry.iter().enumerate() {
            let offset = [(byte >> 4) as i8 - 7, (byte & 15) as i8 - 7];
            ensure!(grid.allowed(i, offset), "Invalid polygon vertex");
            offsets.push(offset);
        }
        let (w, h) = grid.dimensions();
        let colors = FaceColors::decode(colors, 2 * w * h);
        let detail = if tail.is_empty() {
            Vec::new()
        } else {
            let FaceColors::Indexed { indices, .. } = &colors else {
                anyhow::bail!("Polygon detail requires a palette")
            };
            decode_detail(tail, indices)?
        };
        let mesh = Self { grid, offsets, colors, detail };
        ensure!(grid.faces().all(|face| area(mesh.points(face)) > 0), "Folded polygon triangle");
        Ok(mesh)
    }

    /// Maps a face's three shared vertex IDs to screen coordinates.
    fn points(&self, face: Face) -> [Point; 3] {
        face.map(|i| self.grid.point(i, self.offsets[i]))
    }

    /// Counts the filled polygons independently decodable in this picture.
    pub(crate) fn len(&self) -> usize {
        let (w, h) = self.grid.dimensions();
        2 * w * h + 3 * self.detail.iter().flatten().count()
    }

    /// Visits each filled polygon. Child geometry uses half-pixel vertex coordinates.
    fn visit(&self, mut face: impl FnMut([Point; 3], usize, u16)) {
        for (i, ids) in self.grid.faces().enumerate() {
            let triangle = self.points(ids);
            if let (Some(Some(indices)), FaceColors::Indexed { palette, .. }) =
                (self.detail.get(i), &self.colors)
            {
                for (child, &index) in children(triangle).into_iter().zip(indices) {
                    face(child, 2 * self.grid.precision(), palette[usize::from(index)]);
                }
            } else {
                face(triangle, self.grid.precision(), self.colors.at(i));
            }
        }
    }

    /// Expands the mesh into triangles for direct GPU drawing in the live window.
    pub(crate) fn display_triangles(&self) -> Vec<DisplayTriangle> {
        let mut triangles = Vec::with_capacity(self.len());
        self.visit(|tri, scale, color| {
            let width = (scale * WIDTH) as f32;
            let height = (scale * HEIGHT) as f32;
            let points = tri.map(|[x, y]| [x as f32 / width, y as f32 / height]);
            triangles.push(DisplayTriangle { points, rgb: unpack(color) });
        });
        triangles
    }

    /// Scan-converts solid faces to RGB at the receiver.
    pub(crate) fn render(&self, canvas: Canvas) -> Vec<u8> {
        let mut pixels = vec![0; canvas.width * canvas.height * 3];
        self.visit(|tri, scale, color| {
            let rgb = unpack(color);
            spans(tri, scale, canvas, |y, x0, x1| {
                for pixel in pixels[(y * canvas.width + x0) * 3..(y * canvas.width + x1) * 3]
                    .chunks_exact_mut(3)
                {
                    pixel.copy_from_slice(&rgb);
                }
            });
        });
        pixels
    }

    /// Draws the transmitted triangle edges on the offline comparison's source panel.
    pub(crate) fn overlay(&self, source: &[u8], canvas: Canvas) -> Vec<u8> {
        let mut image: Vec<_> = source.iter().map(|v| v / 2).collect();
        let mut mask = vec![0; canvas.width * canvas.height];
        self.visit(|tri, scale, _| {
            let p =
                tri.map(|[x, y]| canvas.project(x as f32 / scale as f32, y as f32 / scale as f32));
            for i in 0..3 {
                crate::video::stroke_segment(&mut mask, canvas, p[i], p[(i + 1) % 3]);
            }
        });
        crate::video::apply_strokes(&mut image, mask, [102, 245, 153]);
        image
    }
}

/// Joins edge midpoints, exactly on the parent edges even for odd vertex coordinates.
fn children([a, b, c]: [Point; 3]) -> [[Point; 3]; 4] {
    let add = |a: Point, b: Point| [a[0] + b[0], a[1] + b[1]];
    let (ab, bc, ca) = (add(a, b), add(b, c), add(c, a));
    [[add(a, a), ab, ca], [ab, add(b, b), bc], [ca, bc, add(c, c)], [ab, bc, ca]]
}

/// Parses the face bitmask and exactly four palette indices for every selected face.
fn decode_detail(data: &[u8], base: &[u8]) -> Result<Vec<Option<[u8; 4]>>> {
    let faces = base.len();
    let (mask, colors) =
        data.split_at_checked(faces.div_ceil(8)).context("Truncated polygon detail mask")?;
    if !faces.is_multiple_of(8) {
        ensure!(mask[mask.len() - 1] >> (faces % 8) == 0, "Invalid polygon detail mask padding");
    }
    let count: usize = mask.iter().map(|m| m.count_ones() as usize).sum();
    ensure!(count > 0 && colors.len() == (3 * count).div_ceil(2), "Invalid polygon detail length");
    ensure!(
        count.is_multiple_of(2) || colors.last().is_some_and(|v| v & 15 == 0),
        "Invalid polygon color padding"
    );
    let mut nibbles = colors.iter().flat_map(|&b| [b >> 4, b & 15]);
    let mut detail = vec![None; faces];
    for (i, entry) in detail.iter_mut().enumerate() {
        if mask[i / 8] & (1 << (i % 8)) != 0 {
            let mut corners = [0; 4];
            for dst in &mut corners[..3] {
                *dst = nibbles.next().context("Missing corner color")?;
            }
            corners[3] = base[i];
            *entry = Some(corners);
        }
    }
    Ok(detail)
}

/// Twice the signed area in canvas pixels, used to rule out folded or vanishing faces.
fn area([a, b, c]: [Point; 3]) -> i32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Visits half-open pixel-center spans so shared edges tile the canvas exactly once.
fn spans(
    triangle: [Point; 3],
    scale: usize,
    canvas: Canvas,
    mut visit: impl FnMut(usize, usize, usize),
) {
    let mut points = triangle.map(|[x, y]| {
        let (px, py) = canvas.project(x as f32 / scale as f32, y as f32 / scale as f32);
        [px, py]
    });
    let first = |v: f32| (v - 0.5).ceil() as usize;
    points.sort_unstable_by(|a, b| a[1].total_cmp(&b[1]));
    let [top, middle, bottom] = points;
    let long_dx = bottom[0] - top[0];
    let long_dy = bottom[1] - top[1];
    for (a, b) in [(top, middle), (middle, bottom)] {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        for y in first(a[1])..first(b[1]) {
            let scan = y as f32 + 0.5;
            let x0 = top[0] + (scan - top[1]) * long_dx / long_dy;
            let x1 = a[0] + (scan - a[1]) * dx / dy;
            let left = first(x0.min(x1));
            let right = first(x0.max(x1));
            if left < right {
                visit(y, left, right);
            }
        }
    }
}

/// Checks that halving the raster preserves its aspect ratio and samples dense meshes adequately.
fn can_reduce_samples(canvas: Canvas, grid: Grid) -> bool {
    let (cols, rows) = grid.dimensions();
    let smaller = sample_canvas(canvas);
    canvas.width.is_multiple_of(SCALE)
        && canvas.height.is_multiple_of(SCALE)
        && smaller.width >= 4 * cols
        && smaller.height >= 4 * rows
}

/// Averages 2×2 RGB blocks to suppress sensor noise and shrink the fitting raster.
fn half_samples(image: &[[u8; 3]], canvas: Canvas, out: &mut Vec<[u8; 3]>) -> Canvas {
    let smaller = sample_canvas(canvas);
    out.clear();
    out.reserve(smaller.width * smaller.height);
    for rows in image.chunks_exact(2 * canvas.width) {
        let (top, bottom) = rows.split_at(canvas.width);
        for (&[a, b], &[c, d]) in top.as_chunks::<2>().0.iter().zip(bottom.as_chunks::<2>().0) {
            out.push(std::array::from_fn(|i| {
                ((u16::from(a[i]) + u16::from(b[i]) + u16::from(c[i]) + u16::from(d[i]) + 2) / 4)
                    as u8
            }));
        }
    }
    smaller
}

/// RGB sums, sample count and visual importance inside a triangle.
struct Region {
    sum: [i64; 3],
    count: i32,
    importance: i32,
    mass: i32,
}

impl Region {
    /// Returns the average sampled RGB color.
    fn mean(&self) -> [u8; 3] {
        let mass = i64::from(self.mass.max(1));
        self.sum.map(|v| ((v + mass / 2) / mass) as u8)
    }

    /// Squared color error with the input-only sum of squares omitted.
    fn error(&self, rgb: [u8; 3]) -> i64 {
        rgb.into_iter()
            .zip(self.sum)
            .map(|(v, s)| {
                let v = i64::from(v);
                v * (v * i64::from(self.mass) - 2 * s)
            })
            .sum()
    }

    /// Squared error saved by fitting the optimal constant color, up to an input-only constant.
    fn fit(&self) -> f64 {
        self.sum.into_iter().map(|v| (v as f64).powi(2)).sum::<f64>() / f64::from(self.mass.max(1))
    }
}

/// Row prefix sums of RGB and visual importance give constant-time statistics per span.
struct Samples {
    canvas: Canvas,
    rows: Vec<[i32; 4]>,
    prioritize_structure: bool,
    weighted: bool,
    precision: usize,
}

impl Samples {
    /// Builds one prefix entry at every sample boundary.
    fn new(
        image: &[[u8; 3]],
        canvas: Canvas,
        importance: Option<&ImportanceMap>,
        precision: usize,
        weighted: bool,
    ) -> Self {
        let mut sums = Vec::with_capacity(canvas.height * (canvas.width + 1));
        for (y, row) in image.chunks_exact(canvas.width).enumerate() {
            let mut sum = [0; 4];
            sums.push(sum);
            for (x, pixel) in row.iter().enumerate() {
                let weight = importance.map_or(1, |map| map.weight(x, y));
                for c in 0..3 {
                    sum[c] += i32::from(pixel[c]) * if weighted { weight } else { 1 };
                }
                sum[3] += weight;
                sums.push(sum);
            }
        }
        Self { canvas, rows: sums, prioritize_structure: importance.is_some(), precision, weighted }
    }

    /// Returns channel sums, pixel count and the total importance of enclosed sample centers.
    fn inside(&self, triangle: [Point; 3], scale: usize) -> Region {
        let (mut sum, mut n, mut importance) = ([0; 3], 0, 0);
        spans(triangle, scale * self.precision, self.canvas, |y, x0, x1| {
            let row = y * (self.canvas.width + 1);
            for (c, total) in sum.iter_mut().enumerate() {
                *total += i64::from(self.rows[row + x1][c] - self.rows[row + x0][c]);
            }
            importance += self.rows[row + x1][3] - self.rows[row + x0][3];
            n += (x1 - x0) as i32;
        });
        Region { sum, count: n, importance, mass: if self.weighted { importance } else { n } }
    }

    /// Samples the centroid when a subpixel face contains no analysis pixel centers.
    fn color_region(&self, triangle: [Point; 3], scale: usize) -> Region {
        let region = self.inside(triangle, scale);
        if region.count > 0 {
            return region;
        }
        let axis = |i: usize, size: usize, extent: usize| {
            let sum: usize = triangle.iter().map(|p| p[i] as usize).sum();
            (sum * size / (3 * scale * self.precision * extent)).min(size - 1)
        };
        let x = axis(0, self.canvas.width, WIDTH);
        let y = axis(1, self.canvas.height, HEIGHT);
        let at = y * (self.canvas.width + 1) + x;
        let sample = |c: usize| self.rows[at + 1][c] - self.rows[at][c];
        let importance = sample(3);
        Region {
            sum: std::array::from_fn(|c| i64::from(sample(c))),
            count: 1,
            importance,
            mass: if self.weighted { importance } else { 1 },
        }
    }
}

/// Restores eight-bit RGB from one transmitted face color.
pub(super) fn unpack(word: u16) -> [u8; 3] {
    let (r, g, b) = ((word >> 11) as u8, ((word >> 5) & 63) as u8, (word & 31) as u8);
    [r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]
}

/// Rounds each channel to the nearest representable RGB565 level.
pub(super) fn pack([r, g, b]: [u8; 3]) -> u16 {
    let level = |value, max| (u16::from(value) * max + 127) / 255;
    (level(r, 31) << 11) | (level(g, 63) << 5) | level(b, 31)
}

/// Packs palette indices high nibble first and zero-pads an unpaired final nibble.
pub(crate) fn write_indices(out: &mut Vec<u8>, mut indices: impl Iterator<Item = u8>) {
    while let Some(high) = indices.next() {
        out.push(high << 4 | indices.next().unwrap_or(0));
    }
}

/// Small grids carry RGB565 directly. Larger grids share sixteen tracked palette entries.
#[derive(Clone)]
enum FaceColors {
    Direct(Vec<u16>),
    Indexed { palette: [u16; 16], indices: Vec<u8> },
}

impl FaceColors {
    /// Looks up the transmitted flat color for a given face ID.
    fn at(&self, index: usize) -> u16 {
        match self {
            Self::Direct(colors) => colors[index],
            Self::Indexed { palette, indices } => palette[usize::from(indices[index])],
        }
    }

    /// Writes the cheaper representation for this grid, packing two indices per byte.
    fn write(&self, out: &mut Vec<u8>, detail: &[Option<[u8; 4]>]) {
        let colors = match self {
            Self::Direct(colors) => colors.as_slice(),
            Self::Indexed { palette, .. } => palette,
        };
        for color in colors {
            out.extend_from_slice(&color.to_le_bytes());
        }
        if let Self::Indexed { indices, .. } = self {
            write_indices(
                out,
                indices.iter().enumerate().map(|(i, &index)| {
                    detail.get(i).and_then(Option::as_ref).map_or(index, |d| d[3])
                }),
            );
        }
    }

    /// Expands an already length-checked grid color section.
    fn decode(bytes: &[u8], count: usize) -> Self {
        if count * 2 <= 32 + count / 2 {
            return Self::Direct(
                bytes.chunks_exact(2).map(|v| u16::from_le_bytes([v[0], v[1]])).collect(),
            );
        }
        let palette = std::array::from_fn(|i| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]));
        let indices = bytes[32..].iter().flat_map(|&v| [v >> 4, v & 15]).collect();
        Self::Indexed { palette, indices }
    }

    /// Runs bounded Lloyd iterations, seeded from the previous palette across adjacent frames.
    fn fit(
        colors: Vec<u16>,
        child_samples: &[[Region; 4]],
        previous: Option<&Self>,
        stabilize: bool,
        update: Update,
    ) -> Self {
        if colors.len() * 2 <= 32 + colors.len() / 2 {
            return Self::Direct(colors);
        }
        let prior_palette = match previous {
            Some(Self::Indexed { palette, .. }) => Some(*palette),
            _ => None,
        };
        let (palette, mut indices) =
            if let Some(old) = prior_palette.filter(|_| update.hold_structure) {
                let palette = old.map(unpack);
                let indices = colors
                    .iter()
                    .map(|&word| {
                        let rgb = unpack(word);
                        palette
                            .iter()
                            .enumerate()
                            .min_by_key(|&(_, &color)| palette::distance(rgb, color))
                            .map_or(0, |(index, _)| index as u8)
                    })
                    .collect();
                (palette, indices)
            } else {
                let samples: Vec<_> = colors
                    .iter()
                    .copied()
                    .map(unpack)
                    .chain(child_samples.iter().flatten().filter(|r| r.count > 0).map(|r| r.mean()))
                    .collect();
                let (palette, mut indices) =
                    palette::fit(&samples, prior_palette, stabilize, update.palette_size);
                indices.truncate(colors.len());
                (palette, indices)
            };
        if let Some(Self::Indexed { indices: old, .. }) = previous.filter(|_| stabilize) {
            for (i, (slot, &prior)) in indices.iter_mut().zip(old).enumerate() {
                let rgb = unpack(colors[i]);
                let regions = &child_samples[i];
                let count = regions.iter().map(|r| r.count).sum::<i32>();
                let weight = regions.iter().map(|r| r.mass).sum::<i32>().max(1);
                let tolerance =
                    (update.color_hysteresis * i64::from(count) / i64::from(weight)) as u32;
                if palette::distance(rgb, palette[usize::from(prior)])
                    <= palette::distance(rgb, palette[usize::from(*slot)]) + tolerance
                {
                    *slot = prior;
                }
            }
        }
        Self::Indexed { palette: palette.map(pack), indices }
    }
}

/// Scores moving a vertex against the fixed opposite edges of its incident triangles.
fn fit_score(
    point: Point,
    edges: &[[Point; 2]],
    min_area: i32,
    mean_area: f64,
    samples: &Samples,
) -> f64 {
    let mut score = 0.0;
    for &[b, c] in edges {
        let tri = [point, b, c];
        if area(tri) < min_area {
            return f64::NEG_INFINITY;
        }
        let region = samples.inside(tri, 1);
        if region.count == 0 {
            return f64::NEG_INFINITY;
        }
        // The input squared-color sum over all incident faces is constant as this vertex moves.
        let flat = region.fit();
        score += flat;
        if samples.prioritize_structure {
            // Predict the extra colors a split can express while fitting geometry near distinctive edges.
            let priority = (f64::from(region.importance) / f64::from(region.count) - 2.0)
                .clamp(0.0, 6.0)
                / 6.0;
            if priority > 0.0 {
                let split: f64 =
                    children(tri).iter().map(|&child| samples.inside(child, 2).fit()).sum();
                score += priority * (split - flat);
            }
            // Encourage adjacent triangles to carry comparable visual importance.
            score -= 16.0 * f64::from(region.importance).powi(2) / mean_area;
        }
    }
    score
}

/// Shared inputs for vertex fitting throughout one picture.
struct VertexFitter<'a> {
    grid: Grid,
    samples: &'a Samples,
    previous: Option<&'a Mesh>,
    adjacent: Vec<Vec<Face>>,
    min_area: i32,
    mean_area: f64,
    movement_limit: i8,
}

impl VertexFitter<'_> {
    /// Tries local offsets while reusing the opposite edges for every candidate.
    fn fit(&self, i: usize, current: Offset, step: i8, offset: impl Fn(usize) -> Offset) -> Offset {
        let faces = &self.adjacent[i];
        let mut edges = [[[0; 2]; 2]; 8];
        for (edge, face) in edges.iter_mut().zip(faces) {
            let at = face.iter().position(|&v| v == i).expect("incident vertex");
            *edge = [face[(at + 1) % 3], face[(at + 2) % 3]].map(|j| self.grid.point(j, offset(j)));
        }
        let origin = self.previous.map_or([0, 0], |m| m.offsets[i]);
        let motion_cost = match (self.previous.is_some(), self.samples.weighted) {
            (true, true) => 1000.0,
            (true, false) => 75.0,
            (false, _) => 3.0,
        };
        let mut best = current;
        let mut best_score = f64::NEG_INFINITY;
        for (dx, dy) in
            [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)]
        {
            let candidate = [current[0] + dx * step, current[1] + dy * step];
            let delta = [candidate[0] - origin[0], candidate[1] - origin[1]];
            if !self.grid.allowed(i, candidate)
                || (self.previous.is_some() && delta.iter().any(|v| v.abs() > self.movement_limit))
            {
                continue;
            }
            let movement = delta.map(|v| i32::from(v).pow(2)).iter().sum::<i32>();
            let score = fit_score(
                self.grid.point(i, candidate),
                &edges[..faces.len()],
                self.min_area,
                self.mean_area,
                self.samples,
            ) - f64::from(movement) * motion_cost;
            if score > best_score {
                best_score = score;
                best = candidate;
            }
        }
        best
    }

    /// Independent rows can be fitted concurrently with the intervening rows held fixed.
    fn parallel_step(&self, offsets: &mut [Offset], step: i8, workers: usize) {
        let (w, h) = self.grid.dimensions();
        let stride = w + 1;
        let band = (h + 1).div_ceil(workers) * stride;
        for parity in 0..2 {
            let frozen = offsets.to_vec();
            std::thread::scope(|scope| {
                for (band_no, group) in offsets.chunks_mut(band).enumerate() {
                    let frozen = &frozen;
                    scope.spawn(move || {
                        for (local_y, row) in group.chunks_exact_mut(stride).enumerate() {
                            let first = band_no * band + local_y * stride;
                            if first / stride % 2 != parity {
                                continue;
                            }
                            for x in 0..stride {
                                row[x] = self.fit(first + x, row[x], step, |j| {
                                    if (first..first + stride).contains(&j) {
                                        row[j - first]
                                    } else {
                                        frozen[j]
                                    }
                                });
                            }
                        }
                    });
                }
            });
        }
    }
}

/// Fits connected vertices by coarse-to-fine coordinate descent around their prior positions.
fn fit_vertices(
    grid: Grid,
    samples: &Samples,
    previous: Option<&Mesh>,
    coarse: bool,
) -> Vec<Offset> {
    let mut offsets = previous.map_or_else(|| vec![[0, 0]; grid.vertices()], |m| m.offsets.clone());
    let mut adjacent = vec![Vec::new(); grid.vertices()];
    for face in grid.faces() {
        for vertex in face {
            adjacent[vertex].push(face);
        }
    }
    let (w, h) = grid.dimensions();
    let fitter = VertexFitter {
        grid,
        samples,
        previous,
        adjacent,
        min_area: (WIDTH * HEIGHT * grid.precision().pow(2) / (8 * w * h)).max(1) as i32,
        mean_area: (samples.canvas.width * samples.canvas.height) as f64 / (2 * w * h) as f64,
        movement_limit: if coarse { 4 } else { 2 },
    };
    let workers = if matches!(grid, Grid::Sized { .. }) {
        std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
    } else {
        1
    };
    let steps: &[i8] = match (coarse, previous.is_some()) {
        (true, true) => &[4, 2],
        (false, true) => &[2, 1],
        (_, false) => &[4, 2, 1],
    };
    for &step in steps {
        if workers > 1 {
            fitter.parallel_step(&mut offsets, step, workers);
        } else {
            for i in 0..offsets.len() {
                offsets[i] = fitter.fit(i, offsets[i], step, |j| offsets[j]);
            }
        }
    }
    offsets
}

/// Ranks subdivisions by the importance-weighted error saved with the transmitted palette.
fn refinements(
    mesh: &Mesh,
    child_samples: &[[Region; 4]],
    previous: Option<&Mesh>,
    limit: usize,
    stabilize: bool,
    update: Update,
) -> Vec<Option<[u8; 4]>> {
    let FaceColors::Indexed { palette, .. } = &mesh.colors else {
        return Vec::new();
    };
    let n = mesh.len();
    let palette = palette.map(unpack);
    let mut candidates = Vec::new();
    let mut detail = if update.hold_structure { vec![None; n] } else { Vec::new() };
    for (i, regions) in child_samples.iter().enumerate() {
        let base = unpack(mesh.colors.at(i));
        let previous_children = previous.and_then(|m| m.detail.get(i)).copied().flatten();
        if update.hold_structure && previous_children.is_none() {
            continue;
        }
        let mut gain = 0i64;
        let mut indices = [0; 4];
        for (k, child) in regions.iter().enumerate() {
            let (index, best) = palette
                .iter()
                .enumerate()
                .map(|(j, &p)| (j, child.error(p)))
                .min_by_key(|&(_, e)| e)
                .expect("nonempty palette");
            let mut slot = index as u8;
            let mut error = best;
            if let Some(old) = previous_children.filter(|_| stabilize) {
                let old_error = child.error(palette[usize::from(old[k])]);
                if old_error <= best + update.color_hysteresis * i64::from(child.count) {
                    slot = old[k];
                    error = old_error;
                }
            }
            indices[k] = slot;
            let saved = child.error(base) - error;
            // The feature objective already weighted each individual source sample.
            gain += if child.mass == child.importance {
                saved
            } else {
                saved * i64::from(child.importance) / i64::from(child.count.max(1))
            };
        }
        if update.hold_structure {
            detail[i] = Some(indices);
        } else if gain > 0 && indices[1..].iter().any(|v| *v != indices[0]) {
            let retained = previous_children.is_some();
            candidates.push((if retained { gain + gain / 4 } else { gain }, i, indices));
        }
    }
    if !candidates.is_empty() {
        detail.resize(n, None);
    }
    candidates.sort_unstable_by_key(|&(score, i, _)| (std::cmp::Reverse(score), i));
    for (_, i, indices) in candidates.into_iter().take(limit) {
        detail[i] = Some(indices);
    }
    if detail.iter().all(Option::is_none) {
        detail.clear();
    }
    detail
}

/// Reverts lower-gain palette changes on faces whose subdivision survived the last picture.
fn retain_color_changes(mesh: &mut Mesh, old: &Mesh, regions: &[[Region; 4]], percent: usize) {
    let (FaceColors::Indexed { palette, indices }, FaceColors::Indexed { indices: prior, .. }) =
        (&mut mesh.colors, &old.colors)
    else {
        return;
    };
    let palette = palette.map(unpack);
    let mut changes = Vec::new();
    for (face, regions) in regions.iter().enumerate() {
        let gain = |region: &Region, from: u8, to: u8| {
            region.error(palette[usize::from(from)]) - region.error(palette[usize::from(to)])
        };
        match (
            mesh.detail.get(face).and_then(Option::as_ref),
            old.detail.get(face).and_then(Option::as_ref),
        ) {
            (Some(next), Some(prior)) => {
                for (child, region) in regions.iter().enumerate() {
                    if next[child] != prior[child] {
                        changes.push((
                            std::cmp::Reverse(gain(region, prior[child], next[child])),
                            face,
                            Some(child),
                            prior[child],
                        ));
                    }
                }
            }
            (None, None) if indices[face] != prior[face] => {
                let total =
                    regions.iter().map(|r| gain(r, prior[face], indices[face])).sum::<i64>();
                changes.push((std::cmp::Reverse(total), face, None, prior[face]));
            }
            _ => {}
        }
    }
    let limit = (changes.len() * percent).div_ceil(100);
    if limit < changes.len() {
        changes.select_nth_unstable(limit);
        for &(_, face, child, previous) in &changes[limit..] {
            if let Some(child) = child {
                if let Some(indices) = &mut mesh.detail[face] {
                    indices[child] = previous;
                }
            } else {
                indices[face] = previous;
            }
        }
    }
}

/// Keeps the previous fitted mesh on the sender. The decoder has no temporal dependencies.
#[derive(Default)]
pub(crate) struct Tracker {
    priority: DetailPriority,
    flat: Option<flat::Merger>,
    importance: Option<ImportanceMap>,
    previous: Option<Mesh>,
    image: Vec<[u8; 3]>,
    configuration: Option<(MeshPlan, Canvas)>,
    low_bandwidth: bool,
    phase: u8,
    smaller_image: Vec<[u8; 3]>,
    last_large_change: bool,
}

impl Tracker {
    /// Creates a mesh tracker with the selected sender-side distortion measure.
    pub(crate) fn new(options: Options) -> Self {
        Self {
            priority: options.priority,
            flat: options.merge_flat_regions.then(flat::Merger::default),
            low_bandwidth: options.low_bandwidth,
            ..Self::default()
        }
    }

    /// Fits and writes a mesh within the selected geometry and subdivision limits.
    pub(crate) fn encode(&mut self, rgb: &[u8], canvas: Canvas, plan: MeshPlan, out: &mut Vec<u8>) {
        let mut samples = Vec::new();
        let analysis = half_samples(rgb.as_chunks::<3>().0, canvas, &mut samples);
        self.encode_samples(&samples, analysis, plan, out);
    }

    /// Fits a mesh directly from a prefiltered RGB raster.
    pub(crate) fn encode_samples(
        &mut self,
        image: &[[u8; 3]],
        analysis: Canvas,
        plan: MeshPlan,
        out: &mut Vec<u8>,
    ) {
        let MeshPlan { grid, splits } = plan;
        let (image, analysis) = if self.low_bandwidth && can_reduce_samples(analysis, grid) {
            let size = half_samples(image, analysis, &mut self.smaller_image);
            (self.smaller_image.as_slice(), size)
        } else {
            (image, analysis)
        };
        let mut difference = 0u32;
        let mut peak = 0;
        for (a, b) in image.iter().zip(&self.image) {
            for c in 0..3 {
                let d = a[c].abs_diff(b[c]);
                difference += u32::from(d);
                peak = peak.max(d);
            }
        }
        let previous = self.previous.as_ref().filter(|_| {
            self.configuration == Some((plan, analysis))
                && difference < (image.len() * 3 * 48) as u32
        });
        if let Some(mesh) = previous.filter(|_| peak <= 2) {
            if let Some(flat) = &self.flat {
                flat.repeat(out);
            } else {
                mesh.write(out);
            }
            return;
        }
        let (period, color_hysteresis, palette_size) = if self.low_bandwidth {
            (STRUCTURE_PERIOD, LOW_BANDWIDTH_COLOR_HYSTERESIS, 12)
        } else {
            (1, COLOR_HYSTERESIS, 16)
        };
        let large_change = difference >= image.len() as u32 * 3 * STRUCTURE_REFRESH_DELTA;
        if previous.is_none() || large_change {
            self.phase = 0;
        }
        let update = Update { hold_structure: self.phase != 0, color_hysteresis, palette_size };
        self.phase = (self.phase + 1) % period;
        let weighted = self.priority == DetailPriority::Features && splits > 0;
        self.importance = (splits > 0 && self.priority != DetailPriority::ColorError).then(|| {
            ImportanceMap::new(image, analysis, previous.and(self.importance.as_ref()), weighted)
        });
        let samples =
            Samples::new(image, analysis, self.importance.as_ref(), grid.precision(), weighted);
        let offsets = if let Some(mesh) = previous.filter(|_| update.hold_structure) {
            mesh.offsets.clone()
        } else {
            fit_vertices(grid, &samples, previous, self.low_bandwidth)
        };
        let colors = grid
            .faces()
            .enumerate()
            .map(|(i, face)| {
                let mut rgb =
                    samples.color_region(face.map(|j| grid.point(j, offsets[j])), 1).mean();
                if let Some(mesh) = previous {
                    let old = unpack(mesh.colors.at(i));
                    if (0..3).all(|c| rgb[c].abs_diff(old[c]) < 16) {
                        rgb = std::array::from_fn(|c| rgb[c].midpoint(old[c]));
                    }
                }
                pack(rgb)
            })
            .collect();
        let child_samples: Vec<_> = grid
            .faces()
            .map(|f| {
                children(f.map(|i| grid.point(i, offsets[i])))
                    .map(|tri| samples.color_region(tri, 2))
            })
            .collect();
        let colors =
            FaceColors::fit(colors, &child_samples, previous.map(|m| &m.colors), weighted, update);
        let mut mesh = Mesh { grid, offsets, colors, detail: Vec::new() };
        let limit = if self.low_bandwidth {
            let percent = if large_change && self.last_large_change { 55 } else { 75 };
            splits * percent / 100
        } else {
            splits
        };
        if limit > 0 {
            mesh.detail = refinements(&mesh, &child_samples, previous, limit, weighted, update);
        }
        if let Some(old) = previous.filter(|_| self.low_bandwidth && update.hold_structure) {
            retain_color_changes(&mut mesh, old, &child_samples, 35);
        }
        self.last_large_change = large_change;
        self.configuration = Some((plan, analysis));
        if let Some(flat) = &mut self.flat {
            flat.write(&mesh, &child_samples, previous.is_some(), out);
        } else {
            mesh.write(out);
        }
        self.image.clear();
        self.image.extend_from_slice(image);
        self.previous = Some(mesh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sparse update budget favors large error reductions within a retained subdivision.
    #[test]
    fn keeps_the_most_valuable_corner_updates() {
        let grid = Grid::Preset(4);
        let faces = grid.face_count();
        let mesh = |corners| {
            let mut palette = [0; 16];
            palette[1] = pack([255; 3]);
            let mut detail = vec![None; faces];
            detail[0] = Some(corners);
            Mesh {
                grid,
                offsets: vec![[0; 2]; grid.vertices()],
                colors: FaceColors::Indexed { palette, indices: vec![0; faces] },
                detail,
            }
        };
        let regions: Vec<_> = (0..faces)
            .map(|_| {
                [210, 10, 250, 169].map(|mean| Region {
                    sum: [mean; 3],
                    count: 1,
                    mass: 1,
                    importance: 1,
                })
            })
            .collect();
        let mut next = mesh([1; 4]);
        retain_color_changes(&mut next, &mesh([0; 4]), &regions, 35);
        assert_eq!(next.detail[0], Some([1, 0, 1, 0]));
    }

    /// Reuses geometry, subdivision identities and palette colors between refreshes.
    #[test]
    fn low_bandwidth_recolors_held_geometry_and_refreshes_on_cuts() {
        let canvas = Canvas::DEFAULT;
        let plan = MeshPlan::for_triangles(100, canvas, DetailPriority::Features, true);
        let mut tracker = Tracker::new(Options { low_bandwidth: true, ..Options::default() });
        let mut mesh_for = |rgb: &[u8]| {
            let mut out = Vec::new();
            tracker.encode(rgb, canvas, plan, &mut out);
            Mesh::decode(&out, canvas).unwrap()
        };
        let frame = |slide| {
            (0..canvas.height)
                .flat_map(|y| {
                    (0..canvas.width).flat_map(move |x| {
                        if (74 + slide..168 + slide).contains(&x) && (32..140).contains(&y) {
                            [209, 109, 36]
                        } else {
                            [54, 86, 138]
                        }
                    })
                })
                .collect::<Vec<_>>()
        };
        let first = mesh_for(&frame(0));
        let first_pixels = first.render(canvas);
        let FaceColors::Indexed { palette: old, .. } = &first.colors else {
            panic!("expected indexed mesh");
        };
        let mask = |mesh: &Mesh| mesh.detail.iter().map(Option::is_some).collect::<Vec<_>>();
        let first_mask = mask(&first);
        assert!(first_mask.contains(&true));
        let mut recolored = false;
        for step in 1..STRUCTURE_PERIOD {
            let held = mesh_for(&frame(10 * usize::from(step)));
            let FaceColors::Indexed { palette, .. } = &held.colors else {
                panic!("expected indexed mesh");
            };
            assert_eq!(held.offsets, first.offsets);
            assert_eq!(palette, old);
            assert_eq!(mask(&held), first_mask);
            recolored |= held.render(canvas) != first_pixels;
        }
        assert!(recolored);

        let cut = [240, 235, 235];
        let picture = mesh_for(&cut.repeat(canvas.width * canvas.height));
        assert!(picture.render(canvas).chunks_exact(3).all(|rgb| rgb == unpack(pack(cut))));
    }

    /// Tiny faces without an analysis sample still display the source color.
    #[test]
    fn subpixel_faces_keep_their_color() {
        let canvas = Canvas::new(64, 36).unwrap();
        let plan = MeshPlan::for_triangles(10_000, canvas, DetailPriority::Features, false);
        let mut body = Vec::new();
        Tracker::default().encode(&[255, 0, 0].repeat(64 * 36), canvas, plan, &mut body);
        let mesh = Mesh::decode(&body, canvas).unwrap();
        assert!(mesh.render(canvas).chunks_exact(3).all(|rgb| rgb == [255, 0, 0]));
    }

    /// Dense topology scales with the triangle limit, and small defaults retain their wire layout.
    #[test]
    fn triangle_plans_bound_base_and_child_faces() {
        let canvas = Canvas::new(1280, 720).unwrap();
        for limit in (2..=1450).chain([10_000, 50_000, u32::MAX]) {
            for priority in [DetailPriority::Structure, DetailPriority::Features] {
                let plan = MeshPlan::for_triangles(limit, canvas, priority, false);
                assert!(plan.max_triangles() <= limit as usize);
                assert!(plan.splits <= plan.grid.face_count());
            }
        }
        assert!(
            MeshPlan::for_triangles(DEFAULT_TRIANGLES, canvas, DetailPriority::Structure, false)
                == (MeshPlan { grid: Grid::Preset(7), splits: 88 })
        );
        assert_eq!(
            MeshPlan::for_triangles(800, canvas, DetailPriority::Structure, false).max_triangles(),
            798
        );
        for n in [1_000, 2_000, 10_000, 50_000] {
            assert!(
                MeshPlan::for_triangles(n, canvas, DetailPriority::Features, false).max_triangles()
                    >= n as usize - 2
            );
        }
    }

    /// Reusing a still picture after lowering the limit cannot leak the old subdivisions.
    #[test]
    fn applies_triangle_limit_to_decoded_frames() {
        let rgb: Vec<_> = (0..WIDTH * HEIGHT)
            .flat_map(|i| {
                let (x, y) = (i % WIDTH, i / WIDTH);
                [(x / 5 * 53) as u8, (y / 7 * 37) as u8, ((x + y) / 6 * 31) as u8]
            })
            .collect();
        let mut tracker = Tracker::default();
        for limit in [800, 120, 31, 2, 432] {
            let plan =
                MeshPlan::for_triangles(limit, Canvas::DEFAULT, DetailPriority::Features, false);
            let mut body = Vec::new();
            tracker.encode(&rgb, Canvas::DEFAULT, plan, &mut body);
            let mesh = Mesh::decode(&body, Canvas::DEFAULT).unwrap();
            assert!(body.len() <= plan.body_bytes() && mesh.len() <= limit as usize);
            if limit >= 120 {
                assert!(mesh.len() > mesh.grid.face_count(), "expected visible child triangles");
            }
            let mut repeat = Vec::new();
            tracker.encode(&rgb, Canvas::DEFAULT, plan, &mut repeat);
            assert_eq!(repeat, body);
        }
    }

    /// A sign away from the image center competes for geometry with a textured background.
    #[test]
    fn structure_priority_concentrates_polygons_on_distinctive_detail() {
        let inside_sign = |x: usize, y: usize| (244..334).contains(&x) && (45..125).contains(&y);
        let rgb: Vec<u8> = (0..WIDTH * HEIGHT)
            .flat_map(|i| {
                let (x, y) = (i % WIDTH, i / WIDTH);
                if inside_sign(x, y) {
                    // Bright sign with two dark glyphs and a narrow border.
                    let border = !(248..330).contains(&x) || !(49..121).contains(&y);
                    let vertical = (259..267).contains(&x) || (283..291).contains(&x);
                    let crossbar = (259..291).contains(&x) && (78..86).contains(&y);
                    let dot = (301..310).contains(&x)
                        && ((62..89).contains(&y) || (97..105).contains(&y));
                    if border || (vertical && (62..105).contains(&y)) || crossbar || dot {
                        [68, 74, 80]
                    } else {
                        [206, 207, 173]
                    }
                } else {
                    let value = [76, 112, 140, 103][(x / 9 + y / 7) % 4];
                    [value, value + 6, value + 12]
                }
            })
            .collect();
        let measure = |priority| {
            let mut packet = Vec::new();
            Tracker::new(Options { priority, ..Options::default() }).encode(
                &rgb,
                Canvas::DEFAULT,
                MeshPlan { grid: Grid::Preset(7), splits: 90 },
                &mut packet,
            );
            let mesh = Mesh::decode(&packet, Canvas::DEFAULT).unwrap();
            let mut polygons = 0;
            mesh.visit(|triangle, scale, _| {
                let centroid = |axis: usize| {
                    triangle.iter().map(|p| p[axis] as usize).sum::<usize>() / (3 * scale)
                };
                polygons += usize::from(inside_sign(centroid(0), centroid(1)));
            });
            let error: u64 = mesh
                .render(Canvas::DEFAULT)
                .chunks_exact(3)
                .zip(rgb.chunks_exact(3))
                .enumerate()
                .filter(|(i, _)| inside_sign(i % WIDTH, i / WIDTH))
                .flat_map(|(_, (a, b))| {
                    a.iter().zip(b).map(|(a, b)| u64::from(a.abs_diff(*b)).pow(2))
                })
                .sum();
            (polygons, error, packet.len())
        };
        let base = measure(DetailPriority::ColorError);
        let focused = measure(DetailPriority::Structure);
        eprintln!("Sign region (polygons, squared RGB error, body bytes): color error {base:?}, structure {focused:?}");
        assert!(
            focused.0 > base.0 && focused.1 < base.1,
            "structure {focused:?}, color error {base:?}"
        );
        assert!(focused.2 <= 378 && base.2 <= 378);
        let features = measure(DetailPriority::Features);
        assert!(
            features.1 < base.1 && features.2 <= 378,
            "features {features:?}, baseline {base:?}"
        );
    }

    /// Builds a moving diagonal two-color boundary with a controlled brightness perturbation.
    fn edge(shift: usize, noise: u8) -> Vec<u8> {
        (0..WIDTH * HEIGHT)
            .flat_map(|i| {
                let x = i % WIDTH;
                let y = i / WIDTH;
                let c = if x > 3 * y / 4 + shift { [200, 54, 87] } else { [46, 130, 75] };
                c.map(|v| v + noise)
            })
            .collect()
    }

    /// A still or slightly noisy input keeps its mesh. Motion is bounded and a lost frame is disposable.
    #[test]
    fn tracks_shapes_and_recovers_from_loss() {
        let mut tracker = Tracker::default();
        let mut encode = |rgb: &[u8]| {
            let mut data = Vec::new();
            tracker.encode(
                rgb,
                Canvas::DEFAULT,
                MeshPlan { grid: Grid::Preset(8), splits: 0 },
                &mut data,
            );
            data
        };
        let first = encode(&edge(54, 0));
        assert_eq!(first, encode(&edge(54, 1)));
        let mut previous = Mesh::decode(&first, Canvas::DEFAULT).unwrap();
        assert_eq!(previous.len(), 224);
        let mut moved = false;
        for shift in [57, 60, 63, 66] {
            let packet = encode(&edge(shift, 0));
            let mesh = Mesh::decode(&packet, Canvas::DEFAULT).unwrap();
            for (a, b) in previous.offsets.iter().zip(&mesh.offsets) {
                moved |= a != b;
                assert!(a[0].abs_diff(b[0]) <= 2 && a[1].abs_diff(b[1]) <= 2);
            }
            // A decoder joining midstream gets the entire canvas, with no gaps or overlap.
            let mut coverage = vec![0u8; WIDTH * HEIGHT];
            for face in mesh.grid.faces() {
                spans(mesh.points(face), 1, Canvas::DEFAULT, |y, x0, x1| {
                    for c in &mut coverage[y * WIDTH + x0..y * WIDTH + x1] {
                        *c += 1;
                    }
                });
            }
            assert!(coverage.iter().all(|&n| n == 1));
            let image = mesh.render(Canvas::DEFAULT);
            let mut error = 0;
            for y in 8..HEIGHT - 8 {
                let row = &image[y * WIDTH * 3..(y + 1) * WIDTH * 3];
                let crossing =
                    row.chunks_exact(3).position(|pixel| pixel[0] > 125).unwrap_or(WIDTH);
                error += crossing.abs_diff(3 * y / 4 + shift);
            }
            assert!(
                error < (HEIGHT - 16) * 6,
                "Moving edge deviated by {} pixels on average",
                error / (HEIGHT - 16)
            );
            previous = mesh;
        }
        assert!(moved);
        // A hard cut must match a cold encode, including its colors.
        let white = vec![255; WIDTH * HEIGHT * 3];
        let mut cold = Vec::new();
        Tracker::default().encode(
            &white,
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(8), splits: 0 },
            &mut cold,
        );
        assert_eq!(encode(&white), cold);
    }

    /// Subdivisions tile the frame, survive packet loss, and remain identical on still input.
    #[test]
    fn refined_faces_are_stable_and_self_contained() {
        let mut tracker = Tracker::default();
        let mut old = Vec::new();
        tracker.encode(
            &edge(54, 0),
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(7), splits: 90 },
            &mut old,
        );
        let rgb = edge(61, 0);
        let mut received = Vec::new();
        tracker.encode(
            &rgb,
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(7), splits: 90 },
            &mut received,
        );
        assert!(received.len() <= 378);
        let mesh = Mesh::decode(&received, Canvas::DEFAULT).unwrap();
        assert!(mesh.detail.iter().any(Option::is_some));
        assert_eq!(
            mesh.render(Canvas::DEFAULT),
            tracker.previous.as_ref().unwrap().render(Canvas::DEFAULT)
        );
        for canvas in
            [Canvas::DEFAULT, Canvas::new(1280, 720).unwrap(), Canvas::new(640, 362).unwrap()]
        {
            let mut coverage = vec![0u8; canvas.width * canvas.height];
            mesh.visit(|tri, scale, _| {
                spans(tri, scale, canvas, |y, l, r| {
                    for count in &mut coverage[y * canvas.width + l..y * canvas.width + r] {
                        *count += 1;
                    }
                })
            });
            assert!(coverage.iter().all(|&n| n == 1), "Shared edge at {canvas:?}");
        }
        let mut still = Vec::new();
        tracker.encode(
            &edge(61, 1),
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(7), splits: 90 },
            &mut still,
        );
        assert_eq!(still, received);
        let cut: Vec<_> = rgb.iter().map(|v| 255 - v).collect();
        let (mut after_cut, mut cold) = (Vec::new(), Vec::new());
        tracker.encode(
            &cut,
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(7), splits: 90 },
            &mut after_cut,
        );
        Tracker::default().encode(
            &cut,
            Canvas::DEFAULT,
            MeshPlan { grid: Grid::Preset(7), splits: 90 },
            &mut cold,
        );
        assert_eq!(after_cut, cold);
        // A bare base is legal. Every partial nonempty extension must be rejected.
        for size in mesh.grid.bytes() + 1..received.len() {
            assert!(Mesh::decode(&received[..size], Canvas::DEFAULT).is_err());
        }
    }

    /// An odd number of refined faces pads the low nibble, and unused mask bits are zero.
    #[test]
    fn validates_packed_detail_indices() {
        let base = [3, 2, 1];
        let decoded = decode_detail(&[2, 0x45, 0x60], &base).unwrap();
        assert_eq!(decoded, [None, Some([4, 5, 6, 2]), None]);
        for bad in
            [&[2, 0x45, 0x61][..], &[0x82, 0x45, 0x60], &[0], &[3, 0x45, 0x60], &[2, 0x45, 0x60, 0]]
        {
            assert!(decode_detail(bad, &base).is_err());
        }
    }

    /// Missing bytes, extra bytes, degenerate border positions and unknown grids are rejected.
    #[test]
    fn rejects_invalid_mesh_packets() {
        let mut packet = vec![0, 0x77, 0x77, 0x77, 0x77, 0, 0, 0, 0];
        assert!(Mesh::decode(&packet, Canvas::DEFAULT).is_ok());
        for n in 0..packet.len() {
            assert!(Mesh::decode(&packet[..n], Canvas::DEFAULT).is_err());
        }
        packet.push(0);
        assert!(Mesh::decode(&packet, Canvas::DEFAULT).is_err());
        packet.pop();
        packet[2] = 0x87;
        assert!(Mesh::decode(&packet, Canvas::DEFAULT).is_err());
        packet[0] = 255;
        assert!(Mesh::decode(&packet, Canvas::DEFAULT).is_err());
    }
}
