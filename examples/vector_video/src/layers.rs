//! Opaque ordered shapes on a shared integer coordinate plane.
mod detail;
mod faces;
mod fine;
mod fit;
mod inference;
mod mesh;
#[cfg(feature = "object-layers")]
mod objects;
#[cfg(feature = "object-layers")]
mod parts;
mod regions;
mod stroke;

use crate::{
    polygon::{pack, unpack, DisplayTriangle},
    video::Canvas,
};
use anyhow::{ensure, Context, Result};
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub(crate) use fit::sample_grid;
pub(crate) use fit::Tracker;
pub(crate) use stroke::is_stroke_record;
use stroke::Stroke;

/// Selects when the encoder runs an optional publisher-side model.
#[derive(Clone, Copy)]
pub(crate) enum InferenceMode {
    Disabled,
    Offline,
    Live,
}

/// Selects the sender's class-agnostic foreground model.
#[derive(Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ObjectProvider {
    /// Use the bundled ONNX foreground model.
    #[default]
    Onnx,
    /// Use Apple Vision foreground instance masks on macOS 14 or later.
    AppleVision,
}

/// Enables each layered-polygon guide independently.
pub(crate) struct Guidance {
    pub(crate) features: bool,
    pub(crate) budgeted_search: bool,
    pub(crate) line_strokes: bool,
    pub(crate) faces: InferenceMode,
    #[cfg(feature = "object-layers")]
    pub(crate) objects: InferenceMode,
    #[cfg(feature = "object-layers")]
    pub(crate) object_provider: ObjectProvider,
    #[cfg(feature = "object-layers")]
    pub(crate) object_model: Option<std::path::PathBuf>,
    #[cfg(feature = "object-layers")]
    pub(crate) parts: Option<(std::path::PathBuf, InferenceMode)>,
}

const W: usize = 192;
const H: usize = 108;
/// Sender analysis dimensions, independent of the stream resolution.
pub(crate) const PLANE: Canvas = Canvas { width: W, height: H };
/// Twice the fitting resolution for measuring narrow feature contrast.
pub(crate) const DETAIL_PLANE: Canvas = Canvas { width: 2 * W, height: 2 * H };
const VERTICES: usize = 8;
pub(crate) const RECORD: usize = 2 + 2 * VERTICES;
/// A byte-predicted body has a 16-bit length, including the background color.
pub(crate) const MAX_LAYERS: u16 = (u16::MAX - 2) / RECORD as u16;
pub(crate) const DEFAULT_LAYERS: u16 = 48;
type Point = [u8; 2];
type Outline = [Point; VERTICES];
type RasterPoint = [f32; 2];

/// One opaque layer and the fitted polygon used to track it across frames.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Patch {
    outline: Outline,
    color: u16,
    stroke: Option<Stroke>,
}

impl Patch {
    /// Creates a filled layer from its closed contour.
    fn polygon(outline: Outline, color: u16) -> Self {
        Self { outline, color, stroke: None }
    }
}

/// Scaled outline and the range of rows in its control polygon's bounds.
struct Edges {
    points: [RasterPoint; VERTICES],
    top: usize,
    bottom: usize,
}

impl Edges {
    /// Projects the outline to the destination raster once per shape.
    fn new(outline: &Outline, canvas: Canvas) -> Self {
        let points = outline.map(|[x, y]| {
            [
                f32::from(x) * canvas.width as f32 / 255.0,
                f32::from(y) * canvas.height as f32 / 255.0,
            ]
        });
        Self::from_points(points)
    }

    /// Builds scanline edges from already projected points.
    fn from_points(points: [RasterPoint; VERTICES]) -> Self {
        let rows = points.map(|p| (p[1] - 0.5).ceil() as usize);
        Self {
            points,
            top: *rows.iter().min().expect("eight vertices"),
            bottom: *rows.iter().max().expect("eight vertices"),
        }
    }

    /// Visits the even-odd fill spans at pixel centers; supports concave outlines.
    fn spans(&self, row: usize, mut visit: impl FnMut(usize, usize)) {
        if row < self.top || row >= self.bottom {
            return;
        }
        let y = row as f32 + 0.5;
        let mut hits = [0usize; VERTICES];
        let mut n = 0;
        let mut hit = |x: f32| {
            hits[n] = (x - 0.5).ceil() as usize;
            n += 1;
        };
        let mut a = self.points[VERTICES - 1];
        for &b in &self.points {
            if (a[1] > y) != (b[1] > y) {
                // Use a consistent direction when the intersection is close to a pixel center.
                let (top, bottom) = if a[1] < b[1] { (a, b) } else { (b, a) };
                hit(top[0] + (bottom[0] - top[0]) * (y - top[1]) / (bottom[1] - top[1]));
            }
            a = b;
        }
        hits[..n].sort_unstable();
        for pair in hits[..n].chunks_exact(2) {
            if pair[0] < pair[1] {
                visit(pair[0], pair[1]);
            }
        }
    }
}

/// Tracks spans already covered by foreground layers, skipping fully painted words.
struct Coverage {
    covered: Vec<u64>,
    next: Vec<usize>,
    last_padding: u64,
    remaining: usize,
}

impl Coverage {
    /// Allocates reusable storage for one scanline with a sentinel after the last word.
    fn new(width: usize) -> Self {
        let words = width.div_ceil(64);
        let last_padding = u64::MAX.checked_shl((width - (words - 1) * 64) as u32).unwrap_or(0);
        Self { covered: vec![0; words], next: vec![0; words + 1], last_padding, remaining: 0 }
    }

    /// Clears the visible part of the scanline and marks pixels beyond the canvas as covered.
    fn reset(&mut self) {
        self.covered.fill(0);
        self.remaining = self.covered.len();
        self.covered[self.remaining - 1] = self.last_padding;
        for (i, next) in self.next.iter_mut().enumerate() {
            *next = i;
        }
    }

    /// Assigns the uncovered runs within a shape's span to its layer.
    fn paint(&mut self, row: &mut [u16], start: usize, end: usize, layer: u16) {
        let mut word = start / 64;
        let last = (end - 1) / 64;
        while word <= last {
            let successor = self.next[word];
            if successor != word {
                self.next[word] = self.next[successor];
                word = self.next[word];
                continue;
            }
            let base = word * 64;
            let low = start.saturating_sub(base);
            let high = (end - base).min(64);
            let mask = (u64::MAX << low) & (u64::MAX >> (64 - high));
            let mut fresh = mask & !self.covered[word];
            self.covered[word] |= mask;
            if self.covered[word] == u64::MAX {
                self.remaining -= 1;
                self.next[word] = self.next[word + 1];
            }
            while fresh != 0 {
                let first = fresh.trailing_zeros();
                let end = first + (fresh >> first).trailing_ones();
                row[base + first as usize..base + end as usize].fill(layer);
                fresh &= u64::MAX.checked_shl(end).unwrap_or(0);
            }
            word += 1;
        }
    }
}

/// One independent background and a back-to-front display list.
#[derive(Default)]
pub(crate) struct Scene {
    background: u16,
    patches: Vec<Patch>,
}

impl Scene {
    /// Maximum raw body size for a user-selected number of layers.
    pub(crate) fn body_bytes(limit: u16) -> usize {
        2 + usize::from(limit) * RECORD
    }

    /// Returns the number of shapes in the display list, including partially hidden shapes.
    pub(crate) fn len(&self) -> usize {
        self.patches.len()
    }

    /// Counts the strokes in the display list.
    pub(crate) fn stroke_count(&self) -> usize {
        self.patches.iter().filter(|patch| patch.stroke.is_some()).count()
    }

    /// Serializes one display list in painter's order.
    fn write(&self, out: &mut Vec<u8>, stroke_records: bool) {
        out.extend_from_slice(&self.background.to_le_bytes());
        for patch in &self.patches {
            out.extend_from_slice(&patch.color.to_le_bytes());
            if let Some(stroke) = patch.stroke {
                stroke.write(out);
                continue;
            }
            let mut outline = patch.outline;
            if stroke_records && outline[0] == outline[1] && outline[1] == outline[2] {
                // Two consecutive duplicate vertices are reserved at the start of stroke records.
                if let Some(first_edge) = outline.windows(2).position(|p| p[0] != p[1]) {
                    outline.rotate_left(first_edge);
                }
            }
            let mut last = [0, 0];
            for point in outline {
                out.push(point[0].wrapping_sub(last[0]));
                out.push(point[1].wrapping_sub(last[1]));
                last = point;
            }
        }
    }

    /// Checks the record count and decodes byte-differenced vertices on the normalized plane.
    pub(crate) fn read(body: &[u8], stroke_records: bool) -> Result<Self> {
        let (color, records) =
            body.split_first_chunk::<2>().context("Missing layered background")?;
        ensure!(
            records.len().is_multiple_of(RECORD) && body.len() <= Self::body_bytes(MAX_LAYERS),
            "Invalid layer record count"
        );
        let mut patches = Vec::with_capacity(records.len() / RECORD);
        for record in records.as_chunks::<RECORD>().0 {
            let color = u16::from_le_bytes([record[0], record[1]]);
            if stroke_records && is_stroke_record(record) {
                patches.push(Patch {
                    outline: [[0; 2]; VERTICES],
                    color,
                    stroke: Some(Stroke::read(record)?),
                });
                continue;
            }
            let mut last = [0u8; 2];
            let outline: Outline = std::array::from_fn(|i| {
                last = [
                    last[0].wrapping_add(record[2 + i * 2]),
                    last[1].wrapping_add(record[3 + i * 2]),
                ];
                last
            });
            patches.push(Patch::polygon(outline, color));
        }
        Ok(Self { background: u16::from_le_bytes(*color), patches })
    }

    /// Resolves occlusion front to back, assigning visible runs in bulk.
    fn owners(&self, canvas: Canvas) -> Vec<u16> {
        let mut edges = Vec::with_capacity(self.patches.len());
        for (i, patch) in self.patches.iter().enumerate() {
            let layer = i as u16 + 1;
            if let Some(stroke) = patch.stroke {
                for quad in stroke.quads(canvas).into_iter().flatten() {
                    let [a, b, c, d] = quad;
                    edges.push((layer, Edges::from_points([a, b, c, d, d, d, d, d])));
                }
            } else {
                edges.push((layer, Edges::new(&patch.outline, canvas)));
            }
        }
        let mut owners = vec![0; canvas.width * canvas.height];
        let mut coverage = Coverage::new(canvas.width);
        for (y, row) in owners.chunks_exact_mut(canvas.width).enumerate() {
            coverage.reset();
            for &(layer, ref edge) in edges.iter().rev() {
                edge.spans(y, |start, end| coverage.paint(row, start, end, layer));
                if coverage.remaining == 0 {
                    break;
                }
            }
        }
        owners
    }

    /// Converts the visible layer at each pixel to RGB.
    fn colors(&self, owners: &[u16]) -> Vec<u8> {
        let colors: Vec<_> = std::iter::once(self.background)
            .chain(self.patches.iter().map(|s| s.color))
            .map(unpack)
            .collect();
        let mut rgb = Vec::with_capacity(3 * owners.len());
        for &owner in owners {
            rgb.extend_from_slice(&colors[usize::from(owner)]);
        }
        rgb
    }

    /// Renders the visible fills for playback or recording.
    pub(crate) fn render(&self, canvas: Canvas) -> Vec<u8> {
        self.colors(&self.owners(canvas))
    }

    /// Builds a resolution-independent draw list in the stream's painter order.
    pub(crate) fn display_triangles(&self, canvas: Canvas) -> Vec<DisplayTriangle> {
        let mut triangles = Vec::with_capacity(2 + self.patches.len() * 6);
        mesh::trapezoid(
            [[0.0, 0.0], [255.0, 0.0], [255.0, 255.0], [0.0, 255.0]],
            unpack(self.background),
            &mut triangles,
        );
        for patch in &self.patches {
            let rgb = unpack(patch.color);
            if let Some(stroke) = patch.stroke {
                for quad in stroke.quads(canvas).into_iter().flatten() {
                    let [a, b, c, d] =
                        quad.map(|[x, y]| [x / canvas.width as f32, y / canvas.height as f32]);
                    triangles.push(DisplayTriangle { points: [a, b, c], rgb });
                    triangles.push(DisplayTriangle { points: [a, c, d], rgb });
                }
            } else {
                mesh::append(&patch.outline, rgb, &mut triangles);
            }
        }
        triangles
    }

    /// Builds a preview using the same resolved coverage for the reconstruction and overlay.
    pub(crate) fn comparison(&self, canvas: Canvas, source: &[u8]) -> Vec<u8> {
        let owners = self.owners(canvas);
        let mut overlay = source.to_vec();
        for (y, row) in owners.chunks_exact(canvas.width).enumerate() {
            let base = y * canvas.width;
            for (x, &owner) in row.iter().enumerate() {
                let i = base + x;
                if (x > 0 && row[x - 1] != owner) || (y > 0 && owners[i - canvas.width] != owner) {
                    overlay[3 * i..3 * i + 3].copy_from_slice(&[70, 255, 115]);
                }
            }
        }
        crate::video::panels(source, &self.colors(&owners), &overlay, canvas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codec::DecodedFrame,
        compression::{Compression, PacketDecoder, PacketEncoder},
    };

    /// Keeps stroke records and closed contours lossless through each packet codec.
    #[test]
    fn stroke_packets_round_trip_without_reordering_the_path() -> anyhow::Result<()> {
        let contour =
            [[16, 18], [16, 18], [16, 18], [61, 18], [61, 73], [16, 73], [16, 73], [16, 73]];
        let frame = |index: u32| {
            let offset = index as u8;
            let line =
                [[25, 45], [140, 45], [140, 53], [25, 53], [25, 53], [25, 53], [25, 53], [25, 53]];
            let line = line.map(|[x, y]| [x + offset, y]);
            let scene = Scene {
                background: 0,
                patches: vec![
                    Patch::polygon(contour, 0x07e0),
                    Patch { outline: line, color: 0xffff, stroke: Stroke::fit(&line) },
                ],
            };
            let mut raw = b"CV\x07".to_vec();
            raw.extend(index.to_le_bytes());
            raw.extend([30, 0, 1, 0, 0x81]);
            scene.write(&mut raw, true);
            (raw, scene.render(Canvas::DEFAULT))
        };
        for mode in [
            Compression::Off,
            Compression::Spatial,
            Compression::Predictive,
            Compression::Compact,
            Compression::CompactMotion,
        ] {
            let mut sender = PacketEncoder::new(mode);
            let mut receiver = PacketDecoder::default();
            for index in 0..3 {
                let (mut raw, expected) = frame(index);
                assert_eq!(raw.len(), 12 + Scene::body_bytes(2));
                sender.stabilize_outline_order(&mut raw);
                let wire = sender.pack(raw.clone())?;
                let decoded = receiver.decode(&wire)?.context("Missing mixed stroke frame")?;
                assert_eq!(decoded.stroke_count(), 1);
                assert_eq!(decoded.render(), expected);
                assert_eq!(receiver.last_snapshot(), Some(raw.as_slice()));
            }
        }
        Ok(())
    }

    /// Reversing a sloped edge through a pixel center preserves the raster fill.
    #[test]
    fn layered_raster_is_independent_of_outline_winding() {
        let canvas = DETAIL_PLANE;
        let outline = [[0, 0], [39, 9], [38, 64], [3, 33], [43, 74], [9, 153], [50, 165], [3, 208]];
        let scene =
            |outline| Scene { background: 0, patches: vec![Patch::polygon(outline, 0xffff)] };
        let mut reversed = outline;
        reversed.reverse();
        assert_eq!(scene(outline).render(canvas), scene(reversed).render(canvas));
    }

    /// Four-corner rectangle with the same fixed-width representation as a traced region.
    fn rect(l: u8, t: u8, r: u8, b: u8, color: u16) -> Patch {
        Patch::polygon([[l, t], [r, t], [r, b], [l, b], [l, b], [l, b], [l, b], [l, b]], color)
    }

    /// Ordered polygons can occlude a silhouette and each other, with background visible outside.
    #[test]
    fn foreground_covers_only_its_own_fill() {
        let canvas = Canvas::new(256, 128).unwrap();
        let scene = Scene {
            background: 0,
            patches: vec![
                rect(12, 12, 220, 220, 0xf800),
                rect(44, 32, 120, 122, 0x07e0),
                rect(72, 88, 180, 180, 0x001f),
            ],
        };
        let mut body = Vec::new();
        scene.write(&mut body, false);
        let decoded = Scene::read(&body, false).unwrap();
        let image = decoded.render(canvas);
        for (x, y, color) in
            [(5, 5, 0), (22, 20, 0xf800), (66, 40, 0x07e0), (100, 49, 0x001f), (150, 70, 0x001f)]
        {
            let at = 3 * (y * canvas.width + x);
            assert_eq!(&image[at..at + 3], unpack(color));
        }
        let concave = Patch {
            outline: [
                [20, 20],
                [160, 20],
                [160, 60],
                [70, 60],
                [70, 160],
                [20, 160],
                [20, 160],
                [20, 160],
            ],
            color: 0x07e0,
            stroke: None,
        };
        let scene = Scene { background: 0, patches: vec![rect(0, 0, 255, 255, 0xf800), concave] };
        let raster = scene.render(canvas);
        let notch = 3 * (50 * canvas.width + 100);
        assert_eq!(&raster[notch..notch + 3], unpack(0xf800));
        assert!(Scene::read(&body[..body.len() - 1], false).is_err());
        assert!(Scene::read(&vec![0; Scene::body_bytes(MAX_LAYERS) + RECORD], false).is_err());
    }

    /// The GPU draw list matches the CPU fill for concave, crossing, and overlapping shapes.
    #[test]
    fn display_mesh_matches_even_odd_coverage() -> anyhow::Result<()> {
        let canvas = Canvas::new(86, 54)?;
        let scene = Scene {
            background: 0x1cf4,
            patches: vec![
                Patch {
                    outline: [
                        [12, 24],
                        [160, 24],
                        [160, 68],
                        [71, 68],
                        [71, 196],
                        [12, 196],
                        [12, 196],
                        [12, 196],
                    ],
                    color: 0xf800,
                    stroke: None,
                },
                Patch {
                    outline: [
                        [39, 51],
                        [224, 209],
                        [34, 209],
                        [223, 51],
                        [223, 51],
                        [223, 51],
                        [223, 51],
                        [223, 51],
                    ],
                    color: 0x07e0,
                    stroke: None,
                },
                rect(175, 70, 219, 119, 0x001f),
                Patch {
                    outline: [[0; 2]; VERTICES],
                    color: 0xffff,
                    stroke: Stroke::fit(&[
                        [20, 55],
                        [86, 70],
                        [145, 53],
                        [145, 62],
                        [86, 79],
                        [20, 64],
                        [20, 64],
                        [20, 64],
                    ]),
                },
                rect(255, 255, 255, 255, 0),
            ],
        };
        assert_eq!(scene.stroke_count(), 1);
        let triangles = scene.display_triangles(canvas);
        let raster = scene.render(canvas);
        let cross = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        };
        for (i, rgb) in raster.chunks_exact(3).enumerate() {
            let (x, y) = (i % canvas.width, i / canvas.width);
            let point =
                [(x as f32 + 0.5) / canvas.width as f32, (y as f32 + 0.5) / canvas.height as f32];
            let triangle = triangles
                .iter()
                .rev()
                .find(|triangle| {
                    let [a, b, c] = triangle.points;
                    cross(a, b, c) > 0.0
                        && [cross(a, b, point), cross(b, c, point), cross(c, a, point)]
                            .iter()
                            .all(|&edge| edge >= -1e-7)
                })
                .context("background triangle missing")?;
            assert_eq!(triangle.rgb, rgb, "pixel ({x}, {y})");
        }
        Ok(())
    }

    /// Occlusion matches painter's order at word boundaries and on an unaligned canvas.
    #[test]
    fn coverage_matches_painter_order() {
        let canvas = Canvas::new(198, 142).unwrap();
        let scene = Scene {
            background: 0,
            patches: vec![
                rect(0, 0, 255, 255, 0xf800),
                rect(37, 14, 252, 238, 0x07e0),
                rect(74, 61, 188, 230, 0x001f),
                rect(149, 0, 251, 195, 0xffe0),
            ],
        };
        let mut expected = vec![0; canvas.width * canvas.height];
        for (i, patch) in scene.patches.iter().enumerate() {
            let edges = Edges::new(&patch.outline, canvas);
            for y in edges.top..edges.bottom {
                let row = &mut expected[y * canvas.width..(y + 1) * canvas.width];
                edges.spans(y, |start, end| row[start..end].fill((i + 1) as u16));
            }
        }
        assert_eq!(scene.owners(canvas), expected);
    }

    /// A lost byte-predicted layer update recovers from the anchor, including bodies over 1 kB.
    #[test]
    fn large_layer_snapshots_recover_after_loss() {
        for mode in
            [Compression::Off, Compression::Spatial, Compression::Predictive, Compression::Compact]
        {
            let mut enc = PacketEncoder::new(mode);
            let mut dec = PacketDecoder::default();
            for i in 0..7u32 {
                let mut raw = b"CV\x07".to_vec();
                raw.extend_from_slice(&i.to_le_bytes());
                raw.extend_from_slice(&[30, 0, 1, 0, 1]);
                let mut scene =
                    Scene { background: 0, patches: vec![rect(20, 30, 160, 180, 0x07e0); 80] };
                scene.patches[68].outline[0][0] += i as u8;
                scene.write(&mut raw, false);
                assert_eq!(DecodedFrame::decode(&raw).unwrap().polygon_count(), 80);
                let wire = enc.pack(raw.clone()).unwrap();
                if i == 1 {
                    continue;
                }
                let received = dec.decode(&wire).unwrap();
                if i == 0 || i >= 4 || matches!(mode, Compression::Off | Compression::Spatial) {
                    let batch = received.unwrap();
                    assert_eq!(batch.start, i);
                    assert_eq!(dec.last_snapshot(), Some(raw.as_slice()));
                    if i == 0 {
                        assert_eq!(batch.render(), scene.render(Canvas::DEFAULT));
                    }
                }
            }
        }
    }

    /// A face-sized patch and its interior marks remain distinct at a small layer budget.
    #[test]
    fn detects_nested_detail_and_respects_a_changed_limit_on_static_input() {
        let mut rgb = Vec::new();
        for y in 0..H {
            for x in 0..W {
                let skin = (40..130).contains(&x) && (20..90).contains(&y);
                let eye =
                    ((58..67).contains(&x) || (97..106).contains(&x)) && (40..46).contains(&y);
                let lip = (75..96).contains(&x) && (69..74).contains(&y);
                rgb.extend_from_slice(if eye {
                    &[16, 16, 16]
                } else if lip {
                    &[125, 38, 42]
                } else if skin {
                    &[210, 155, 120]
                } else {
                    &[30, 80, 130]
                });
            }
        }
        let mut tracker = Tracker::default();
        let mut sparse = Vec::new();
        tracker.encode(&rgb, PLANE, 1, &mut sparse);
        let mut detail = Vec::new();
        tracker.encode(&rgb, PLANE, 16, &mut detail);
        let scene = Scene::read(&detail, false).unwrap();
        assert!((4..=16).contains(&scene.len()));
        let frame = scene.render(PLANE);
        for (x, y) in [(55, 65), (63, 43), (103, 42), (85, 71), (20, 23)] {
            let i = 3 * (y * W + x);
            assert!(frame[i..i + 3].iter().zip(&rgb[i..i + 3]).all(|(a, b)| a.abs_diff(*b) < 12));
        }
        let mut repeat = Vec::new();
        tracker.encode(&rgb, PLANE, 16, &mut repeat);
        assert_eq!(repeat, detail);
    }

    /// Pools a textured wall and joins adjacent similar fills while retaining a dark detail.
    #[test]
    fn merges_similar_background_colors() -> anyhow::Result<()> {
        let mut input = Vec::new();
        for y in 0..H {
            for x in 0..W {
                let wall = 104 + ((x / 6 + y / 5) % 3) as u8 * 5;
                let color = if (58..72).contains(&x) && (45..53).contains(&y) {
                    [32, 23, 31]
                } else if (30..150).contains(&x) && (24..84).contains(&y) {
                    let shade = if x < 90 { 0 } else { 10 };
                    [169 + shade, 153 + shade, 132 + shade]
                } else {
                    [wall, wall + 5, wall + 8]
                };
                input.extend_from_slice(&color);
            }
        }
        let mut tracker = Tracker::default();
        let mut bytes = Vec::new();
        tracker.encode(&input, PLANE, 64, &mut bytes);
        let scene = Scene::read(&bytes, false)?;
        assert_eq!(scene.len(), 2);
        let owners = scene.owners(PLANE);
        let owner = |x: usize, y: usize| owners[y * W + x];
        assert_eq!(owner(3, 9), 0);
        assert_eq!(owner(177, 93), 0);
        assert_eq!(owner(40, 60), 1);
        assert_eq!(owner(140, 60), 1);
        assert_eq!(owner(64, 49), 2);
        Ok(())
    }
}
