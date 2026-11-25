use std::{fmt, str::FromStr, time::Duration};

use anyhow::{bail, ensure, Context, Result};
use clap::ValueEnum;

use crate::{
    layers, polygon,
    video::{self, AnalyzedFrame, Canvas},
};

const MAGIC: &[u8; 2] = b"CV";
const SIZED_PACKET: u8 = 0x80;
const DIMENSION_BYTES: usize = 4;
const HEADER_BYTES: usize = 2 + 1 + 4 + 2 + 2 + 1;
const STROKE_FRAME: u8 = 0x81;
/// Decoder allocation bound from the largest legal raster, including geometry and split colors.
pub(crate) const MAX_FRAME_BYTES: usize = 2 * Canvas::MAX_PIXELS + 2 * 8192 + 128;
pub(crate) const DEFAULT_OVERHEAD: usize = 144;

/// Selects the representation used to encode and render a snapshot.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum WireFormat {
    /// Adaptive flat-colored triangles with extra detail in high-error regions.
    ColoredPolygon = 6,
    /// Opaque overlapping polygons painted from the background toward the foreground.
    LayeredPolygon = 7,
}

impl TryFrom<u8> for WireFormat {
    type Error = anyhow::Error;

    /// Validates the version of either supported polygon format.
    fn try_from(version: u8) -> Result<Self> {
        match version {
            6 => Ok(Self::ColoredPolygon),
            7 => Ok(Self::LayeredPolygon),
            _ => bail!("Unsupported vector video format version {version}"),
        }
    }
}

/// A constant frame rate expressed as an exact rational number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameRate(u16, u16);

impl FrameRate {
    pub(crate) const POLYGON_DEFAULT: Self = Self(15, 1);
    #[cfg(test)]
    pub(crate) const CAMERA_DEFAULT: Self = Self(30, 1);

    /// Validates the integer frame rate negotiated by a native camera.
    pub(crate) fn from_camera(fps: u32) -> Result<Self> {
        Self::new(u16::try_from(fps)?, 1)
    }

    /// Returns the camera request rate if the value is a whole number.
    pub(crate) fn whole(self) -> Option<u32> {
        self.0.is_multiple_of(self.1).then(|| u32::from(self.0 / self.1))
    }

    /// Checks a rate read from the command line or the wire.
    fn new(numerator: u16, denominator: u16) -> Result<Self> {
        ensure!(denominator != 0, "FPS denominator must be nonzero");
        ensure!(
            numerator >= denominator && u32::from(numerator) <= 60 * u32::from(denominator),
            "FPS must be between 1 and 60"
        );
        Ok(Self(numerator, denominator))
    }

    /// Converts a number of frames to a duration, rounded up to a nanosecond.
    pub(crate) fn duration(self, frames: u64) -> Duration {
        let nanos =
            (1_000_000_000 * u128::from(self.1) * u128::from(frames)).div_ceil(u128::from(self.0));
        Duration::new((nanos / 1_000_000_000) as u64, (nanos % 1_000_000_000) as u32)
    }
}

impl FromStr for FrameRate {
    type Err = anyhow::Error;

    /// Parses integer or rational FPS such as `30` or `30000/1001`.
    fn from_str(value: &str) -> Result<Self> {
        let (numerator, denominator) = value.split_once('/').unwrap_or((value, "1"));
        Self::new(
            numerator.parse().context("Invalid FPS numerator")?,
            denominator.parse().context("Invalid FPS denominator")?,
        )
    }
}

impl fmt::Display for FrameRate {
    /// Formats the rate for FFmpeg's rawvideo input.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.0, self.1)
    }
}

/// Geometry limits for one self-contained picture.
enum Geometry {
    Triangles(polygon::MeshPlan),
    Layers(u16),
}

/// A cap on the bytes carried in one picture, with its geometry and bandwidth accounting.
pub(crate) struct Budget {
    pub(crate) canvas: Canvas,
    pub(crate) payload: usize,
    pub(crate) overhead: usize,
    geometry: Geometry,
}

/// Returns the full snapshot header size from the version byte.
pub(crate) fn header_len(version: u8) -> usize {
    HEADER_BYTES + if version & SIZED_PACKET != 0 { DIMENSION_BYTES } else { 0 }
}

/// Identifies a layered header that allows typed stroke records.
pub(crate) fn has_stroke_records(header: &[u8]) -> bool {
    header.get(11) == Some(&STROKE_FRAME)
}

/// Reads canvas dimensions from a full snapshot header, also used by compressed keyframes.
pub(crate) fn packet_canvas(header: &[u8]) -> Result<Canvas> {
    let &version = header.get(2).context("Missing packet version")?;
    if version & SIZED_PACKET == 0 {
        return Ok(Canvas::DEFAULT);
    }
    let dims = header.get(HEADER_BYTES..HEADER_BYTES + 4).context("Truncated video dimensions")?;
    Canvas::new(u16::from_le_bytes([dims[0], dims[1]]), u16::from_le_bytes([dims[2], dims[3]]))
}

/// Checks incoming sizes against the geometry limits for the snapshot's format.
pub(crate) fn check_size(version: u8, len: usize, canvas: Canvas) -> Result<()> {
    let max = match WireFormat::try_from(version & !SIZED_PACKET)? {
        WireFormat::ColoredPolygon => {
            // At most one grid cell per analysis sample, plus the boundary vertices and header.
            1000.max(2 * canvas.width * canvas.height + canvas.width + canvas.height + 128)
        }
        WireFormat::LayeredPolygon => {
            header_len(version) + layers::Scene::body_bytes(layers::MAX_LAYERS)
        }
    };
    ensure!(len <= max, "Invalid vector frame length");
    Ok(())
}

impl Budget {
    /// Derives the frame size from the configured mesh.
    pub(crate) fn for_triangles(plan: polygon::MeshPlan, canvas: Canvas, overhead: usize) -> Self {
        let payload = HEADER_BYTES + Self::dimension_bytes(canvas) + plan.body_bytes();
        Self { canvas, payload, overhead, geometry: Geometry::Triangles(plan) }
    }

    /// Sizes an independently decodable display list using the requested shape count.
    pub(crate) fn for_layers(limit: u16, canvas: Canvas, overhead: usize) -> Self {
        let payload =
            HEADER_BYTES + Self::dimension_bytes(canvas) + layers::Scene::body_bytes(limit);
        Self { canvas, payload, overhead, geometry: Geometry::Layers(limit) }
    }

    /// Returns the wire format associated with the geometry.
    pub(crate) fn format(&self) -> WireFormat {
        match self.geometry {
            Geometry::Triangles(_) => WireFormat::ColoredPolygon,
            Geometry::Layers(_) => WireFormat::LayeredPolygon,
        }
    }

    /// Counts the optional width and height fields.
    fn dimension_bytes(canvas: Canvas) -> usize {
        if canvas == Canvas::DEFAULT {
            0
        } else {
            DIMENSION_BYTES
        }
    }
}

/// A complete decoded picture with optional camera exposure metadata from its Data Track frame.
pub(crate) struct DecodedFrame {
    pub(crate) exposure_us: Option<u64>,
    pub(crate) canvas: Canvas,
    pub(crate) start: u32,
    pub(crate) fps: FrameRate,
    picture: Picture,
}

/// Selects the decoded geometry carried by a validated picture.
enum Picture {
    Polygons(polygon::Mesh),
    Layers(layers::Scene),
}

/// Retains temporal fitting state across independently decodable pictures.
#[derive(Default)]
pub(crate) struct Encoder {
    polygons: polygon::Tracker,
    layers: layers::Tracker,
}

impl Encoder {
    /// Configures triangle refinement and optional model guidance for layered polygons.
    pub(crate) fn new(options: polygon::Options, guidance: layers::Guidance) -> Result<Self> {
        Ok(Self {
            polygons: polygon::Tracker::new(options),
            layers: layers::Tracker::new(guidance)?,
        })
    }

    /// Writes one budgeted snapshot for the selected representation.
    pub(crate) fn encode(
        &mut self,
        start: u32,
        fps: FrameRate,
        budget: &Budget,
        frame: &AnalyzedFrame,
    ) -> Vec<u8> {
        let mut data = header(start, fps, budget);
        match budget.geometry {
            Geometry::Layers(limit) => {
                if self.layers.uses_strokes() {
                    data[11] = STROKE_FRAME;
                }
                let (pixels, size) = frame.analysis(budget.canvas);
                self.layers.encode(pixels, size, limit, &mut data)
            }
            Geometry::Triangles(plan) => {
                if let Some((pixels, size)) = frame.mesh_samples() {
                    self.polygons.encode_samples(pixels, size, plan, &mut data);
                } else {
                    let (pixels, size) = frame.analysis(budget.canvas);
                    self.polygons.encode(pixels, size, plan, &mut data);
                }
            }
        }
        data
    }
}

/// Opens a single-picture packet using the existing version 6 or version 7 header.
fn header(start: u32, fps: FrameRate, budget: &Budget) -> Vec<u8> {
    let mut data = Vec::with_capacity(budget.payload);
    data.extend_from_slice(MAGIC);
    let sized = budget.canvas != Canvas::DEFAULT;
    data.push(budget.format() as u8 | if sized { SIZED_PACKET } else { 0 });
    data.extend_from_slice(&start.to_le_bytes());
    data.extend_from_slice(&fps.0.to_le_bytes());
    data.extend_from_slice(&fps.1.to_le_bytes());
    data.push(1);
    if sized {
        data.extend_from_slice(&(budget.canvas.width as u16).to_le_bytes());
        data.extend_from_slice(&(budget.canvas.height as u16).to_le_bytes());
    }
    data
}

impl DecodedFrame {
    /// Parses and validates one complete polygon snapshot.
    pub(crate) fn decode(data: &[u8]) -> Result<Self> {
        let canvas = packet_canvas(data)?;
        ensure!(data.starts_with(MAGIC), "Unsupported vector video packet");
        let version = data[2];
        let format = WireFormat::try_from(version & !SIZED_PACKET)?;
        check_size(version, data.len(), canvas)?;
        let (header, body) =
            data.split_at_checked(header_len(version)).context("Truncated video header")?;
        let start = u32::from_le_bytes(header[3..7].try_into()?);
        let fps = FrameRate::new(
            u16::from_le_bytes(header[7..9].try_into()?),
            u16::from_le_bytes(header[9..11].try_into()?),
        )?;
        let strokes = format == WireFormat::LayeredPolygon && has_stroke_records(header);
        ensure!(header[11] == 1 || strokes, "Invalid polygon frame count");
        let picture = match format {
            WireFormat::ColoredPolygon => Picture::Polygons(polygon::Mesh::decode(body, canvas)?),
            WireFormat::LayeredPolygon => Picture::Layers(layers::Scene::read(body, strokes)?),
        };
        Ok(Self { exposure_us: None, start, fps, canvas, picture })
    }

    /// Counts decoded triangles or painted layers.
    pub(crate) fn polygon_count(&self) -> usize {
        match &self.picture {
            Picture::Polygons(mesh) => mesh.len(),
            Picture::Layers(scene) => scene.len(),
        }
    }

    /// Counts open strokes in the picture.
    pub(crate) fn stroke_count(&self) -> usize {
        match &self.picture {
            Picture::Polygons(_) => 0,
            Picture::Layers(scene) => scene.stroke_count(),
        }
    }

    /// Returns flat-colored triangles for GPU playback.
    pub(crate) fn display_triangles(&self) -> Vec<polygon::DisplayTriangle> {
        match &self.picture {
            Picture::Polygons(mesh) => mesh.display_triangles(),
            Picture::Layers(scene) => scene.display_triangles(self.canvas),
        }
    }

    /// Rasterizes one frame on the CPU for recording.
    pub(crate) fn render(&self) -> Vec<u8> {
        match &self.picture {
            Picture::Polygons(mesh) => mesh.render(self.canvas),
            Picture::Layers(scene) => scene.render(self.canvas),
        }
    }

    /// Composes the offline three-panel view and counts its visible shapes.
    pub(crate) fn comparison(&self, source: &[u8]) -> (Vec<u8>, usize) {
        let image = match &self.picture {
            Picture::Layers(scene) => scene.comparison(self.canvas, source),
            Picture::Polygons(mesh) => video::panels(
                source,
                &mesh.render(self.canvas),
                &mesh.overlay(source, self.canvas),
                self.canvas,
            ),
        };
        (image, self.polygon_count())
    }
}
