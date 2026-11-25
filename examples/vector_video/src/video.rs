use std::{ffi::OsStr, io, path::Path, process::Stdio};

use anyhow::{bail, ensure, Context, Result};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
};

use crate::codec::{FrameRate, WireFormat};

pub(crate) const WIDTH: usize = 384;
pub(crate) const HEIGHT: usize = 216;
/// Dimensions of one RGB picture. Wire and CLI sizes are validated before allocating frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Canvas {
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Canvas {
    pub(crate) const MAX_PIXELS: usize = 4096 * 2160;
    pub(crate) const DEFAULT: Self = Self { width: WIDTH, height: HEIGHT };

    /// Checks the even dimensions required by the MP4 sink and bounds decoded memory.
    pub(crate) fn new(width: u16, height: u16) -> Result<Self> {
        let (width, height) = (usize::from(width), usize::from(height));
        ensure!((2..=8192).contains(&width) && (2..=8192).contains(&height)
            && width.is_multiple_of(2) && height.is_multiple_of(2)
            && width * height <= Self::MAX_PIXELS,
            "Video dimensions must be even, between 2 and 8192 per axis, and at most 4096×2160 pixels in total (got {width}×{height})");
        Ok(Self { width, height })
    }

    /// Converts a point on the logical codec canvas to this raster's pixel coordinates.
    pub(crate) fn project(self, x: f32, y: f32) -> (f32, f32) {
        (x * self.width as f32 / WIDTH as f32, y * self.height as f32 / HEIGHT as f32)
    }
}
/// The camera worker sends this in place of FPS before a length-prefixed error message.
pub(crate) const CAMERA_STARTUP_ERROR: u32 = 0;

/// Creates a media subprocess without inheriting LiveKit credentials.
pub(crate) fn media_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    for name in ["LIVEKIT_API_KEY", "LIVEKIT_API_SECRET", "LIVEKIT_TOKEN"] {
        command.env_remove(name);
    }
    command
}

/// A source frame analyzed on arrival while the camera is acquiring the next one.
pub(crate) struct AnalyzedFrame {
    pub(crate) rgb: Vec<u8>,
    pub(crate) captured_us: u64,
    pub(crate) exposure_us: Option<u64>,
    analysis_rgb: Option<Vec<u8>>,
    capture_analysis: Option<Canvas>,
    pub(crate) analysis_time: std::time::Duration,
}

impl AnalyzedFrame {
    /// Makes a frame at the default canvas size for codec tests.
    #[cfg(test)]
    pub(crate) fn new(rgb: Vec<u8>) -> Self {
        Self::from_capture(
            rgb,
            crate::timing::now_us(),
            WireFormat::LayeredPolygon,
            Canvas::DEFAULT,
            None,
        )
    }

    /// Gives small triangle canvases a fixed analysis raster while retaining the original RGB.
    fn from_capture(
        rgb: Vec<u8>,
        captured_us: u64,
        format: WireFormat,
        canvas: Canvas,
        capture_analysis: Option<Canvas>,
    ) -> Self {
        let start = std::time::Instant::now();
        let canonical = format == WireFormat::ColoredPolygon
            && (canvas.width < WIDTH || canvas.height < HEIGHT);
        let analysis_rgb = (capture_analysis.is_none() && canonical && canvas != Canvas::DEFAULT)
            .then(|| {
                let image = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(
                    canvas.width as u32,
                    canvas.height as u32,
                    rgb.as_slice(),
                )
                .expect("source reads a complete RGB canvas");
                image::imageops::thumbnail(&image, WIDTH as u32, HEIGHT as u32).into_raw()
            });
        Self {
            rgb,
            analysis_rgb,
            captured_us,
            exposure_us: None,
            capture_analysis,
            analysis_time: start.elapsed(),
        }
    }

    /// Returns the RGB analysis raster and its dimensions.
    pub(crate) fn analysis(&self, canvas: Canvas) -> (&[u8], Canvas) {
        if let Some(size) = self.capture_analysis {
            return (&self.rgb, size);
        }
        self.analysis_rgb.as_deref().map_or((&self.rgb, canvas), |rgb| (rgb, Canvas::DEFAULT))
    }

    /// Returns camera pixels that have already been reduced to the mesh analysis raster.
    pub(crate) fn mesh_samples(&self) -> Option<(&[[u8; 3]], Canvas)> {
        self.capture_analysis.map(|size| (self.rgb.as_chunks::<3>().0, size))
    }
}

/// Owns a capture or FFmpeg process that reads fixed-size RGB frames.
pub(crate) struct Source {
    child: Child,
    stdout: ChildStdout,
    name: &'static str,
    camera_timestamps: bool,
    canvas: Canvas,
    capture_analysis: Option<Canvas>,
}

impl Source {
    /// Starts decoding a file or FFmpeg input at the selected cadence.
    pub(crate) fn file(
        input: &str,
        input_format: Option<&str>,
        fps: FrameRate,
        canvas: Canvas,
    ) -> Result<Self> {
        let Canvas { width, height } = canvas;
        let filter = format!("fps={fps},scale={width}:{height}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1");
        let mut command = media_command("ffmpeg");
        command.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        if let Some(format) = input_format {
            command.args(["-f", format]);
        }
        command.args([
            "-i",
            input,
            "-map",
            "0:V:0",
            "-vf",
            &filter,
            "-fps_mode",
            "cfr",
            "-pix_fmt",
            "rgb24",
            "-threads:v",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
        Self::spawn(command, "FFmpeg source", canvas)
    }

    /// Starts an RGB producer and takes ownership of its output pipe.
    pub(crate) fn spawn(mut command: Command, name: &'static str, canvas: Canvas) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Could not start {name}"))?;
        let stdout = child.stdout.take().with_context(|| format!("{name} pipe unavailable"))?;
        Ok(Self { child, stdout, name, camera_timestamps: false, canvas, capture_analysis: None })
    }

    /// Matches the pipe's image dimensions to the camera worker's polygon preprocessing.
    pub(crate) fn set_capture_analysis(&mut self, capture_analysis: Option<Canvas>) {
        self.capture_analysis = capture_analysis;
    }

    /// Reads the capture worker's startup error or its negotiated FPS.
    pub(crate) async fn camera_rate(&mut self) -> Result<FrameRate> {
        let fps = match self.stdout.read_u32_le().await {
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                let status = self.child.wait().await?;
                bail!("Camera capture exited before reporting a mode ({status})");
            }
            result => result.context("Could not read camera startup status")?,
        };
        if fps == CAMERA_STARTUP_ERROR {
            let length = self.stdout.read_u16_le().await.context("Missing camera error length")?;
            let mut message = vec![0; usize::from(length)];
            self.stdout.read_exact(&mut message).await.context("Incomplete camera error")?;
            bail!("{}", String::from_utf8_lossy(&message));
        }
        self.camera_timestamps = true;
        FrameRate::from_camera(fps)
    }

    /// Reads one RGB frame and checks the decoder's exit status at EOF.
    pub(crate) async fn read_frame(&mut self, format: WireFormat) -> Result<Option<AnalyzedFrame>> {
        let size = self.capture_analysis.unwrap_or(self.canvas);
        let mut rgb = vec![0; size.width * size.height * 3];
        let mut stamp = [0u8; 8];
        let first = if self.camera_timestamps { &mut stamp[..1] } else { &mut rgb[..1] };
        if self.stdout.read(first).await? == 0 {
            let status = self.child.wait().await?;
            ensure!(status.success(), "{} failed ({status})", self.name);
            return Ok(None);
        }
        let (captured_us, exposure_us, rest) = if self.camera_timestamps {
            self.stdout.read_exact(&mut stamp[1..]).await.context("Truncated camera timestamp")?;
            let exposure =
                self.stdout.read_u64_le().await.context("Truncated exposure timestamp")?;
            (u64::from_le_bytes(stamp), (exposure != 0).then_some(exposure), &mut rgb[..])
        } else {
            (crate::timing::now_us(), None, &mut rgb[1..])
        };
        self.stdout.read_exact(rest).await.context("Truncated source video frame")?;
        let mut frame = AnalyzedFrame::from_capture(
            rgb,
            captured_us,
            format,
            self.canvas,
            self.capture_analysis,
        );
        frame.exposure_us = exposure_us;
        Ok(Some(frame))
    }

    /// Native capture delivers frames at the device clock, including fractional timing jitter.
    pub(crate) fn is_camera(&self) -> bool {
        self.camera_timestamps
    }

    /// Stops decoding and reaps the child if the caller exits early.
    pub(crate) async fn stop(&mut self) -> Result<()> {
        self.child.kill().await.with_context(|| format!("Could not stop {}", self.name))
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use crate::{
        codec::{Budget, DecodedFrame, Encoder, DEFAULT_OVERHEAD},
        polygon,
    };

    /// Every wire mode carries dimensions, fits its packet budget and fills a nondefault canvas.
    #[test]
    fn sized_packets_render_the_advertised_canvas() {
        // Half-resolution polygon analysis also exercises partially filled 4×4 importance tiles.
        let canvas = Canvas::new(640, 362).unwrap();
        let fps = FrameRate::POLYGON_DEFAULT;
        let plan = polygon::MeshPlan::for_triangles(
            polygon::DEFAULT_TRIANGLES,
            canvas,
            polygon::DetailPriority::default(),
            false,
        );
        for budget in [
            Budget::for_triangles(plan, canvas, DEFAULT_OVERHEAD),
            Budget::for_layers(crate::layers::DEFAULT_LAYERS, canvas, DEFAULT_OVERHEAD),
        ] {
            let format = budget.format();
            let frame = AnalyzedFrame::from_capture(
                vec![255; canvas.width * canvas.height * 3],
                0,
                format,
                canvas,
                None,
            );
            let mut bytes = Encoder::default().encode(7, fps, &budget, &frame);
            assert!(bytes.len() <= budget.payload);
            let batch = DecodedFrame::decode(&bytes).unwrap();
            assert_eq!(batch.canvas, canvas);
            let rgb = batch.render();
            assert_eq!(rgb.len(), canvas.width * canvas.height * 3);
            assert!(rgb.iter().all(|&v| v == 255));
            assert!(DecodedFrame::decode(&bytes[..15]).is_err());
            bytes[12..16].fill(0xff);
            assert!(DecodedFrame::decode(&bytes).is_err());
        }
    }
}

#[cfg(all(test, unix))]
mod source_tests {
    use super::*;

    /// Reports a worker's useful startup error instead of a truncated frame-rate header.
    #[tokio::test]
    async fn forwards_camera_startup_error() {
        let mut command = Command::new("sh");
        command.args(["-c", r"printf '\000\000\000\000\010\000Rejected'"]);
        let mut source = Source::spawn(command, "test capture worker", Canvas::DEFAULT).unwrap();
        assert_eq!(source.camera_rate().await.unwrap_err().to_string(), "Rejected");
        assert!(source.child.wait().await.unwrap().success());
    }

    /// The camera worker timestamp precedes pixels and survives conversion and pipe delay.
    #[tokio::test]
    async fn preserves_camera_capture_timestamp() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            r"printf '\036\000\000\000\052\000\000\000\000\000\000\000\041\000\000\000\000\000\000\000'; head -c 248832 /dev/zero",
        ]);
        let mut source =
            Source::spawn(command, "timestamped capture worker", Canvas::DEFAULT).unwrap();
        source.camera_rate().await.unwrap();
        let frame = source.read_frame(WireFormat::LayeredPolygon).await.unwrap().unwrap();
        assert_eq!(frame.captured_us, 42);
        assert_eq!(frame.exposure_us, Some(33));
        assert!(source.read_frame(WireFormat::LayeredPolygon).await.unwrap().is_none());
    }

    /// Stops and reaps a capture process even if it never produces another frame.
    #[tokio::test]
    async fn stops_blocked_capture_worker() {
        let mut command = Command::new("sh");
        command.args(["-c", r"printf '\036\000\000\000'; exec sleep 60"]);
        let mut source = Source::spawn(command, "test capture worker", Canvas::DEFAULT).unwrap();
        assert_eq!(source.camera_rate().await.unwrap(), FrameRate::CAMERA_DEFAULT);
        assert!(source.child.try_wait().unwrap().is_none());
        source.stop().await.unwrap();
        assert!(source.child.try_wait().unwrap().is_some());
    }
}

/// Owns the FFmpeg process recording reconstructed RGB frames.
pub(crate) struct Sink {
    child: Child,
    stdin: ChildStdin,
}

impl Sink {
    /// Opens an MP4 encoder with the selected panel count and frame rate.
    pub(crate) fn open(
        output: &Path,
        fps: FrameRate,
        canvas: Canvas,
        preview: bool,
    ) -> Result<Self> {
        let width = canvas.width * if preview { 3 } else { 1 };
        let height = canvas.height;
        let mut command = media_command("ffmpeg");
        command.args(["-hide_banner", "-loglevel", "error"]);
        command.args([
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgb24",
            "-video_size",
            &format!("{width}x{height}"),
            "-framerate",
            &fps.to_string(),
            "-i",
            "pipe:0",
        ]);
        command
            .args([
                "-n",
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(output);
        let mut child = command
            .stdin(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Could not start FFmpeg")?;
        let stdin = child.stdin.take().context("Video output pipe unavailable")?;
        Ok(Self { child, stdin })
    }

    /// Writes one RGB frame to the video output.
    pub(crate) async fn write(&mut self, frame: &[u8]) -> io::Result<()> {
        self.stdin.write_all(frame).await
    }

    /// Flushes the output video and reaps its encoder.
    pub(crate) async fn finish(mut self) -> Result<()> {
        drop(self.stdin);
        ensure!(self.child.wait().await?.success(), "FFmpeg output failed");
        Ok(())
    }

    /// Waits for the encoder to exit independently of frame arrival.
    pub(crate) async fn wait(&mut self) -> Result<()> {
        ensure!(self.child.wait().await?.success(), "Video output failed");
        Ok(())
    }
}

/// Places source, reconstruction and received geometry side by side.
pub(crate) fn panels(source: &[u8], decoded: &[u8], overlay: &[u8], canvas: Canvas) -> Vec<u8> {
    let mut image = Vec::with_capacity(source.len() * 3);
    for y in 0..canvas.height {
        let row = y * canvas.width * 3..(y + 1) * canvas.width * 3;
        image.extend_from_slice(&source[row.clone()]);
        image.extend_from_slice(&decoded[row.clone()]);
        image.extend_from_slice(&overlay[row]);
    }
    image
}

/// Linearly interpolates between two color samples.
fn blend(a: f32, b: f32, weight: f32) -> f32 {
    a + (b - a) * weight
}

/// Blends a coverage mask into an RGB image.
pub(crate) fn apply_strokes(image: &mut [u8], mask: Vec<u8>, color: [u8; 3]) {
    for (pixel, coverage) in image.chunks_exact_mut(3).zip(mask) {
        if coverage == 0 {
            continue;
        }
        let rim = f32::from(coverage.min(100)) * 0.0075;
        let core = f32::from(coverage.saturating_sub(100).min(100)) * 0.01;
        for (sample, ink) in pixel.iter_mut().zip(color) {
            *sample =
                blend(blend(f32::from(*sample), 22.0, rim), f32::from(ink), core).round() as u8;
        }
    }
}

/// Rasterizes a round-ended segment into a union mask, visiting only the narrow stroke.
pub(crate) fn stroke_segment(mask: &mut [u8], canvas: Canvas, a: (f32, f32), b: (f32, f32)) {
    let steep = (b.1 - a.1).abs() > (b.0 - a.0).abs();
    let axes = |(x, y)| if steep { (y, x) } else { (x, y) };
    let (mut a, mut b) = (axes(a), axes(b));
    if a.0 > b.0 {
        std::mem::swap(&mut a, &mut b);
    }
    let (width, height) =
        if steep { (canvas.height, canvas.width) } else { (canvas.width, canvas.height) };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let slope = if dx == 0.0 { 0.0 } else { dy / dx };
    let length2 = (dx * dx + dy * dy).max(1.0);
    for x in
        ((a.0 - 2.25).ceil() as i32).max(0)..=((b.0 + 2.25).floor() as i32).min(width as i32 - 1)
    {
        let center = a.1 + slope * (x as f32 - a.0).clamp(0.0, dx);
        for y in ((center - 3.25).ceil() as i32).max(0)
            ..=((center + 3.25).floor() as i32).min(height as i32 - 1)
        {
            let (px, py) = (x as f32 - a.0, y as f32 - a.1);
            let t = ((px * dx + py * dy) / length2).clamp(0.0, 1.0);
            let distance2 = (px - t * dx).powi(2) + (py - t * dy).powi(2);
            if distance2 >= 2.25 * 2.25 {
                continue;
            }
            let coverage = ((2.25 - distance2.sqrt()) * 100.0).round() as u8;
            let index = if steep {
                x as usize * canvas.width + y as usize
            } else {
                y as usize * canvas.width + x as usize
            };
            mask[index] = mask[index].max(coverage);
        }
    }
}
