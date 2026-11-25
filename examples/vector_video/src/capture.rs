#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
#[path = "../../local_video/src/capture_timestamp.rs"]
mod capture_timestamp;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
mod gpu;

use anyhow::{ensure, Context, Result};
use clap::ValueEnum;
use tokio::signal;

use crate::{
    codec::{FrameRate, WireFormat},
    layers, polygon,
    video::{media_command, Canvas, Source},
};

const NATIVE_CAMERA: bool =
    cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows"));

/// Requested camera settings before the backend negotiates a capture format.
pub(crate) struct CameraOptions {
    pub(crate) camera_index: u32,
    pub(crate) requested: Option<Canvas>,
    pub(crate) mode: WireFormat,
    pub(crate) face_layers: bool,
    pub(crate) feature_layers: bool,
}

impl CameraOptions {
    /// Keeps the default hardware request separate from the default encoded image.
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    fn device_size(&self) -> Canvas {
        self.requested.unwrap_or(Canvas { width: 640, height: 480 })
    }

    /// Resolves the advertised stream canvas, independently of the capture raster.
    fn canvas(&self) -> Canvas {
        self.requested.unwrap_or(Canvas::DEFAULT)
    }

    /// Sends just the mesh analysis raster when capture can prepare it directly.
    fn mesh_samples(&self) -> Option<Canvas> {
        let canvas = self.canvas();
        (self.mode == WireFormat::ColoredPolygon
            && canvas.width >= Canvas::DEFAULT.width
            && canvas.height >= Canvas::DEFAULT.height)
            .then(|| polygon::sample_canvas(canvas))
    }

    /// Sends the layered analysis plane unless facial detail needs source-resolution pixels.
    fn layered_samples(&self) -> Option<Canvas> {
        let canvas = self.canvas();
        let detailed = self.feature_layers
            && canvas.width >= layers::DETAIL_PLANE.width
            && canvas.height >= layers::DETAIL_PLANE.height;
        let samples = if detailed { layers::DETAIL_PLANE } else { layers::PLANE };
        (self.mode == WireFormat::LayeredPolygon
            && !self.face_layers
            && canvas.width >= layers::PLANE.width
            && canvas.height >= layers::PLANE.height)
            .then_some(samples)
    }
}

/// Starts a native camera reader and waits for its negotiated FPS.
pub(crate) async fn open(
    options: &CameraOptions,
    fps: FrameRate,
    prefix: &[&str],
) -> Result<Option<(FrameRate, Source)>> {
    ensure!(
        NATIVE_CAMERA,
        "Native camera capture requires macOS, Linux, or Windows. Use --input with FFmpeg."
    );
    let fps = fps.whole().context("Native camera capture requires a whole-number --fps")?;
    // Camera backends can block inside a frame read. Own the reader as a child process
    // so shutdown can kill and reap it even if the camera has stopped responding.
    let mut command = media_command(std::env::current_exe()?);
    command.args(prefix).args([
        "--capture-worker",
        "--camera-index",
        &options.camera_index.to_string(),
        "--fps",
        &fps.to_string(),
    ]);
    if let Some(size) = options.requested {
        command.args(["--width", &size.width.to_string(), "--height", &size.height.to_string()]);
    }
    let mode = options.mode.to_possible_value().context("Missing camera worker mode")?;
    command.args(["--mode", mode.get_name()]);
    if options.face_layers {
        command.arg("--face-layers");
    }
    if options.feature_layers {
        command.arg("--feature-layers");
    }
    let mut source = Source::spawn(command, "camera capture", options.canvas())?;
    source.set_capture_analysis(options.mesh_samples().or_else(|| options.layered_samples()));
    let ready = tokio::select! {
        fps = source.camera_rate() => fps.map(Some),
        result = signal::ctrl_c() => result.map(|()| None).map_err(Into::into),
    };
    match ready {
        Ok(Some(fps)) => Ok(Some((fps, source))),
        result => {
            let stopped = source.stop().await;
            result?;
            stopped?;
            Ok(None)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod native {
    use anyhow::{bail, Result};

    use super::{CameraOptions, FrameRate};

    /// Reports that the native camera backend is unavailable.
    pub(crate) fn list_cameras() -> Result<()> {
        bail!("Native camera capture requires macOS, Linux, or Windows")
    }

    /// Reports that the native camera worker is unavailable.
    pub(crate) fn run_worker(_options: &CameraOptions, _fps: FrameRate) -> Result<()> {
        list_cameras()
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
mod native {
    use std::{
        io::{self, Write},
        sync::mpsc::{self, Receiver, SyncSender, TrySendError},
        sync::Mutex,
        thread,
        time::{Duration, Instant},
    };

    use anyhow::{anyhow, ensure, Context, Result};
    use image::RgbImage;
    use nokhwa::{
        pixel_format::RgbFormat,
        utils::{
            ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat,
            RequestedFormatType, Resolution,
        },
        Buffer, Camera,
    };

    #[cfg(test)]
    use super::WireFormat;
    use super::{gpu::MeshSampler, CameraOptions, FrameRate};
    use crate::layers;
    use crate::video::{Canvas, CAMERA_STARTUP_ERROR};
    #[cfg(test)]
    use crate::video::{HEIGHT, WIDTH};

    /// Prints the camera indices and names reported by the host backend.
    pub(crate) fn list_cameras() -> Result<()> {
        let cameras = nokhwa::query(ApiBackend::Auto).context("Could not enumerate cameras")?;
        if cameras.is_empty() {
            println!("No cameras found");
        }
        for camera in cameras {
            println!("{}  {}", camera.index(), camera.human_name());
        }
        Ok(())
    }

    /// Captures native RGB video and writes the FPS followed by fixed-size frames to stdout.
    pub(crate) fn run_worker(options: &CameraOptions, fps: FrameRate) -> Result<()> {
        let fps = fps.whole().context("Native camera capture requires a whole-number --fps")?;
        #[cfg(target_os = "macos")]
        if let Err(error) = initialize_camera() {
            return write_startup_error(&error);
        }
        let mut camera = match try_capture_modes(options, fps, |mode| {
            let request = RequestedFormat::new::<RgbFormat>(mode);
            let mut camera = Camera::new(CameraIndex::Index(options.camera_index), request)?;
            prefer_camera_rate(&mut camera, options, fps)?;
            camera.open_stream().context("Could not start camera stream")?;
            FrameRate::from_camera(camera.frame_rate())?;
            Ok(camera)
        }) {
            Ok(camera) => camera,
            Err(error) => return write_startup_error(&error),
        };
        let format = camera.camera_format();
        eprintln!(
            "Camera {} opened at {}x{} @ {} fps ({})",
            options.camera_index,
            format.width(),
            format.height(),
            format.frame_rate(),
            format.format()
        );
        let input_fps = format.frame_rate();
        let output_fps = fps.min(input_fps);
        let result = write_frames(&mut camera, input_fps, output_fps, options);
        let stopped = camera.stop_stream().context("Could not stop camera stream");
        result.and(stopped)
    }

    /// Resolves AVFoundation authorization before attempting to lock or configure the device.
    #[cfg(target_os = "macos")]
    fn initialize_camera() -> Result<()> {
        let (reply, permission) = mpsc::sync_channel(1);
        nokhwa::nokhwa_initialize(move |granted| {
            let _ = reply.try_send(granted);
        });
        ensure!(permission.recv().context("Camera permission request ended")?,
            "macOS denied camera access. Enable Camera for the app running the publisher in System Settings > Privacy & Security.");
        Ok(())
    }

    /// Ranks modes by deliverable cadence, requested dimensions and conversion preference.
    fn mode_rank(mode: CameraFormat, options: &CameraOptions, fps: u32) -> (u32, u32, u32, bool) {
        let size = options.device_size();
        let delta = mode.frame_rate().abs_diff(fps);
        // A polygon camera can decimate a faster 720p mode to the requested cadence.
        let cadence =
            if options.requested.is_some() { fps.saturating_sub(mode.frame_rate()) } else { delta };
        (
            cadence,
            mode.width().abs_diff(size.width as u32) + mode.height().abs_diff(size.height as u32),
            delta,
            mode.format() != FrameFormat::YUYV,
        )
    }

    /// Tries advertised modes with a better cadence or geometry before accepting a fallback.
    fn prefer_camera_rate(camera: &mut Camera, options: &CameraOptions, fps: u32) -> Result<()> {
        let initial = camera.camera_format();
        let rank = |mode| mode_rank(mode, options, fps);
        let initial_rank = rank(initial);
        if initial.frame_rate() == fps && (options.requested.is_none() || initial_rank.1 == 0) {
            return Ok(());
        }
        let Ok(mut modes) = camera.compatible_camera_formats() else { return Ok(()) };
        modes.retain(|&mode| (1..=60).contains(&mode.frame_rate()) && rank(mode) < initial_rank);
        modes.sort_by_key(|&mode| rank(mode));
        modes.dedup();
        for mode in modes {
            let request = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(mode));
            if camera.set_camera_requset(request).is_ok() {
                return Ok(());
            }
        }
        if camera.camera_format() != initial {
            camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
                RequestedFormatType::Exact(initial),
            ))?;
        }
        Ok(())
    }

    /// Opens directly in a usable mode and falls back if configuring or starting it fails.
    fn try_capture_modes<T>(
        options: &CameraOptions,
        fps: u32,
        mut try_open: impl FnMut(RequestedFormatType) -> Result<T>,
    ) -> Result<T> {
        use RequestedFormatType::{
            AbsoluteHighestFrameRate, Closest, Exact, HighestFrameRate, HighestResolution,
            None as AnyFormat,
        };

        let requested = options.device_size();
        let size = Resolution::new(requested.width as u32, requested.height as u32);
        let mut requests = vec![];
        for format in [
            FrameFormat::YUYV,
            FrameFormat::MJPEG,
            FrameFormat::NV12,
            FrameFormat::RAWRGB,
            FrameFormat::RAWBGR,
        ] {
            let mode = CameraFormat::new(size, format, fps);
            // AVFoundation applies the format in the constructor. Start with an
            // advertised mode to avoid configuring an unsupported default or guess.
            requests.push(if cfg!(target_os = "macos") { Closest(mode) } else { Exact(mode) });
        }
        for fallback in [
            HighestFrameRate(fps),
            HighestFrameRate(fps.saturating_sub(1).max(1)),
            HighestResolution(size),
            HighestFrameRate(30),
            HighestFrameRate(29),
            AbsoluteHighestFrameRate,
            AnyFormat,
        ] {
            if !requests.contains(&fallback) {
                requests.push(fallback);
            }
        }
        let mut first_error = None;
        for request in requests {
            match try_open(request) {
                Ok(camera) => return Ok(camera),
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        Err(first_error.unwrap_or_else(|| anyhow!("No capture modes available"))).with_context(
            || {
                format!(
                    "Could not open camera {} at {}x{} @ {fps} fps or in a fallback mode",
                    options.camera_index, requested.width, requested.height
                )
            },
        )
    }

    /// Sends a startup failure to the parent in place of a frame rate.
    fn write_startup_error(error: &anyhow::Error) -> Result<()> {
        let message = format!("{error:#}");
        let length = u16::try_from(message.len())?;
        let mut output = io::stdout().lock();
        output.write_all(&CAMERA_STARTUP_ERROR.to_le_bytes())?;
        output.write_all(&length.to_le_bytes())?;
        output.write_all(message.as_bytes())?;
        Ok(output.flush()?)
    }

    /// Holds one replaceable image while the writer is blocked on the camera pipe.
    type LatestCapture = Mutex<Option<(u64, Option<u64>, RgbImage)>>;

    /// Keeps reading camera frames even while the publisher's pipe is full.
    fn write_frames(
        camera: &mut Camera,
        input_fps: u32,
        fps: u32,
        options: &CameraOptions,
    ) -> Result<()> {
        FrameRate::from_camera(fps).context("The negotiated camera rate is outside 1–60 fps")?;
        let (tx, rx) = mpsc::sync_channel(1);
        let latest = LatestCapture::default();
        thread::scope(|scope| {
            let writer = scope.spawn(|| write_pipe(rx, &latest, fps));
            let captured = capture_frames(camera, tx, &latest, input_fps, fps, options);
            let written = writer.join().map_err(|_| anyhow!("Camera pipe writer panicked"))?;
            captured.and(written)
        })
    }

    /// Replaces a waiting frame and wakes the pipe writer when the slot changes.
    fn capture_frames(
        camera: &mut Camera,
        tx: SyncSender<()>,
        latest: &LatestCapture,
        input_fps: u32,
        fps: u32,
        options: &CameraOptions,
    ) -> Result<()> {
        let samples = options.mesh_samples();
        let layered = options.layered_samples();
        let canvas = samples.unwrap_or_else(|| options.canvas());
        let format = camera.camera_format();
        let mut gpu = samples
            .filter(|size| {
                size.width * size.height >= 640 * 360
                    && format.width() as usize == size.width * 2
                    && format.height() as usize == size.height * 2
                    && format.format() == FrameFormat::YUYV
            })
            .and_then(|size| {
                match tokio::runtime::Handle::current().block_on(MeshSampler::new(size)) {
                    Ok(Some(sampler)) => {
                        eprintln!(
                            "Using GPU YUYV conversion for {}x{} mesh samples",
                            size.width, size.height
                        );
                        Some(sampler)
                    }
                    Ok(None) => {
                        eprintln!("Using CPU mesh sampling (no hardware GPU adapter)");
                        None
                    }
                    Err(error) => {
                        eprintln!("Using CPU mesh sampling: {error:#}");
                        None
                    }
                }
            });
        let mut timing_available = None;
        let mut phase = input_fps - fps;
        let mut last_report = Instant::now();
        let mut captured = 0;
        let mut dropped = 0;
        let mut conversion = Duration::ZERO;
        loop {
            let capture = camera.frame().context("Could not read camera frame")?;
            // Retain the requested cadence even when the backend only offers a faster mode.
            phase += fps;
            if phase < input_fps {
                continue;
            }
            phase -= input_fps;
            let captured_us = crate::timing::now_us();
            let exposure_us = capture.capture_timestamp().and_then(|stamp| {
                super::capture_timestamp::validate_backend_capture_timestamp_us(stamp, captured_us)
                    .ok()
            });
            if timing_available != Some(exposure_us.is_some()) {
                eprintln!(
                    "{}",
                    if exposure_us.is_some() {
                        "Using camera capture_timestamp for Data Track user_timestamp"
                    } else {
                        "Camera exposure timestamp unavailable; g2g unavailable"
                    }
                );
                timing_available = Some(exposure_us.is_some());
            }
            let started = Instant::now();
            let frame = if let Some(analysis) = layered {
                layered_canvas(&capture, canvas, analysis)?
            } else {
                match samples.and_then(|_| packed_mesh_input(&capture, canvas)) {
                    Some(bytes) => convert_mesh_samples(bytes, canvas, &mut gpu)?,
                    None => camera_canvas(&capture, canvas)?,
                }
            };
            conversion += started.elapsed();
            if latest
                .lock()
                .expect("capture handoff poisoned")
                .replace((captured_us, exposure_us, frame))
                .is_some()
            {
                dropped += 1;
            }
            match tx.try_send(()) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => return Ok(()),
            }
            captured += 1;
            let elapsed = last_report.elapsed();
            if elapsed >= Duration::from_secs(5) {
                eprintln!(
                    "Camera {:.1} fps after resize, {:.1} ms/frame converting, {dropped} frames dropped at the capture queue",
                    f64::from(captured) / elapsed.as_secs_f64(), conversion.as_secs_f64() * 1000.0 / f64::from(captured)
                );
                last_report = Instant::now();
                captured = 0;
                dropped = 0;
                conversion = Duration::ZERO;
            }
        }
    }

    /// Uses a GPU when available and falls back permanently if its readback fails.
    fn convert_mesh_samples(
        bytes: &[u8],
        canvas: Canvas,
        gpu: &mut Option<MeshSampler>,
    ) -> Result<RgbImage> {
        if let Some(sampler) = gpu {
            match sampler.sample(bytes) {
                Ok(frame) => return Ok(frame),
                Err(error) => {
                    eprintln!("Using CPU mesh sampling after GPU conversion failed: {error:#}");
                    *gpu = None;
                }
            }
        }
        cpu_mesh_samples(bytes, canvas)
    }

    /// Converts each packed 2×2 block straight to one mesh sample, preserving RGB rounding.
    fn cpu_mesh_samples(bytes: &[u8], canvas: Canvas) -> Result<RgbImage> {
        let stride = canvas.width * 4;
        let mut pixels = Vec::with_capacity(canvas.width * canvas.height * 3);
        for rows in bytes.chunks_exact(stride * 2) {
            let (top, bottom) = rows.split_at(stride);
            for (top, bottom) in top.as_chunks::<4>().0.iter().zip(bottom.as_chunks::<4>().0) {
                let convert = |&[y0, u, y1, v]: &[u8; 4]| {
                    [
                        nokhwa::utils::yuyv444_to_rgb(y0.into(), u.into(), v.into()),
                        nokhwa::utils::yuyv444_to_rgb(y1.into(), u.into(), v.into()),
                    ]
                };
                let [a, b] = convert(top);
                let [c, d] = convert(bottom);
                for i in 0..3 {
                    pixels.push(
                        ((u16::from(a[i])
                            + u16::from(b[i])
                            + u16::from(c[i])
                            + u16::from(d[i])
                            + 2)
                            / 4) as u8,
                    );
                }
            }
        }
        RgbImage::from_raw(canvas.width as u32, canvas.height as u32, pixels)
            .context("Invalid camera mesh sample dimensions")
    }

    /// Checks for tightly packed YUYV with one pair per output column.
    fn packed_mesh_input(capture: &Buffer, canvas: Canvas) -> Option<&[u8]> {
        let res = capture.resolution();
        let bytes = capture.buffer();
        (capture.source_frame_format() == FrameFormat::YUYV
            && res.width() as usize == canvas.width * 2
            && res.height() as usize == canvas.height * 2
            && bytes.len() == canvas.width * canvas.height * 8)
            .then_some(bytes)
    }

    /// Writes the capture metadata and each accepted RGB frame to the parent process.
    fn write_pipe(rx: Receiver<()>, latest: &LatestCapture, fps: u32) -> Result<()> {
        let mut output = io::stdout().lock();
        output.write_all(&fps.to_le_bytes())?;
        output.flush()?;
        for () in rx {
            let Some((stamp, exposure, frame)) =
                latest.lock().expect("capture handoff poisoned").take()
            else {
                continue;
            };
            if let Err(error) = output
                .write_all(&stamp.to_le_bytes())
                .and_then(|()| output.write_all(&exposure.unwrap_or(0).to_le_bytes()))
                .and_then(|()| output.write_all(frame.as_raw()))
                .and_then(|()| output.flush())
            {
                if error.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(());
                }
                return Err(error.into());
            }
        }
        Ok(())
    }

    /// Converts only the packed camera pixels that contribute to the layered analysis plane.
    fn layered_canvas(capture: &Buffer, canvas: Canvas, analysis: Canvas) -> Result<RgbImage> {
        let size = capture.resolution();
        let exact =
            size.width() as usize == canvas.width && size.height() as usize == canvas.height;
        let pixels = if let Some(bytes) = packed_yuyv(capture).filter(|_| exact) {
            layers::sample_grid(canvas, analysis, |x, y| {
                yuyv_rgb(packed_pixel(bytes, canvas.width, x, y))
            })
        } else {
            let rgb = camera_canvas(capture, canvas)?;
            layers::sample_grid(canvas, analysis, |x, y| rgb.get_pixel(x as u32, y as u32).0)
        };
        RgbImage::from_raw(analysis.width as u32, analysis.height as u32, pixels.into_flattened())
            .context("Invalid layered camera sample dimensions")
    }

    /// Accepts packed YUYV only when complete chroma pairs occupy exactly the frame's buffer.
    fn packed_yuyv(capture: &Buffer) -> Option<&[u8]> {
        let size = capture.resolution();
        let (width, height) = (size.width() as usize, size.height() as usize);
        let bytes = capture.buffer();
        (capture.source_frame_format() == FrameFormat::YUYV
            && width.is_multiple_of(2)
            && width.checked_mul(height).and_then(|n| n.checked_mul(2)) == Some(bytes.len()))
        .then_some(bytes)
    }

    /// Looks up luma at a pixel and the chroma shared by its packed YUYV pair.
    fn packed_pixel(bytes: &[u8], width: usize, x: usize, y: usize) -> [u8; 3] {
        let pair = (y * width + (x & !1)) * 2;
        [bytes[pair + (x & 1) * 2], bytes[pair + 1], bytes[pair + 3]]
    }

    /// Uses the same YUYV rounding and clipping for sampled and full camera frames.
    fn yuyv_rgb([y, u, v]: [u8; 3]) -> [u8; 3] {
        nokhwa::utils::yuyv444_to_rgb(y.into(), u.into(), v.into())
    }

    /// Downsamples packed YUYV before converting to RGB so work is bounded by the output size.
    fn camera_canvas(capture: &Buffer, canvas: Canvas) -> Result<RgbImage> {
        let (width, height) =
            (capture.resolution().width() as usize, capture.resolution().height() as usize);
        if let Some(bytes) = packed_yuyv(capture) {
            return sample_canvas(
                canvas,
                width as u64,
                height as u64,
                |x, y| packed_pixel(bytes, width, x, y),
                yuyv_rgb,
            );
        }
        letterbox(
            canvas,
            capture.decode_image::<RgbFormat>().context("Could not decode camera frame to RGB")?,
        )
    }

    /// Fits decoded RGB pixels into the codec canvas.
    fn letterbox(canvas: Canvas, rgb: RgbImage) -> Result<RgbImage> {
        let (width, height) = rgb.dimensions();
        if (width, height) == (canvas.width as u32, canvas.height as u32) {
            return Ok(rgb);
        }
        sample_canvas(
            canvas,
            width.into(),
            height.into(),
            |x, y| rgb.get_pixel(x as u32, y as u32).0,
            |rgb| rgb,
        )
    }

    /// Samples four points per output pixel and centers the image on a black canvas.
    fn sample_canvas(
        size: Canvas,
        width: u64,
        height: u64,
        sample_pixel: impl Fn(usize, usize) -> [u8; 3],
        convert: impl Fn([u8; 3]) -> [u8; 3],
    ) -> Result<RgbImage> {
        ensure!(width > 0 && height > 0, "Camera returned an empty frame");
        let (out_width, out_height) = (size.width as u64, size.height as u64);
        let scaled_width = out_width.min(width * out_height / height).max(1);
        let scaled_height = out_height.min(height * out_width / width).max(1);
        let x_start = (out_width - scaled_width) as usize / 2;
        let y_start = (out_height - scaled_height) as usize / 2;
        let sample = |position, source, target| {
            let offset = position * 4;
            (
                ((offset + 1) * source / (4 * target)) as usize,
                ((offset + 3) * source / (4 * target)) as usize,
            )
        };
        let columns: Vec<_> = (0..scaled_width).map(|x| sample(x, width, scaled_width)).collect();
        let mut canvas = RgbImage::new(size.width as u32, size.height as u32);
        let buffer: &mut [u8] = canvas.as_mut();
        for (y, row) in buffer
            .chunks_exact_mut(size.width * 3)
            .skip(y_start)
            .take(scaled_height as usize)
            .enumerate()
        {
            let (top, bottom) = sample(y as u64, height, scaled_height);
            for (pixel, &(left, right)) in row[x_start * 3..].chunks_exact_mut(3).zip(&columns) {
                let a = sample_pixel(left, top);
                let b = sample_pixel(right, top);
                let c = sample_pixel(left, bottom);
                let d = sample_pixel(right, bottom);
                let mean = std::array::from_fn(|i| {
                    ((u16::from(a[i]) + u16::from(b[i]) + u16::from(c[i]) + u16::from(d[i]) + 2)
                        / 4) as u8
                });
                pixel.copy_from_slice(&convert(mean));
            }
        }
        Ok(canvas)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// An HD camera running faster than the polygon stream keeps its requested resolution.
        #[test]
        fn prefers_hd_camera_at_decimatable_rate() {
            let canvas = Canvas::new(1280, 720).unwrap();
            let options = CameraOptions {
                camera_index: 0,
                requested: Some(canvas),
                mode: WireFormat::ColoredPolygon,
                face_layers: false,
                feature_layers: false,
            };
            let hd = CameraFormat::new(Resolution::new(1280, 720), FrameFormat::YUYV, 30);
            let sd = CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 15);
            assert!(mode_rank(hd, &options, 15) < mode_rank(sd, &options, 15));
        }

        /// Keeps trying usable advertised modes after the requested geometry is rejected.
        #[test]
        fn retries_camera_startup_after_rejected_formats() {
            let options = CameraOptions {
                camera_index: 0,
                requested: None,
                mode: WireFormat::LayeredPolygon,
                face_layers: false,
                feature_layers: false,
            };
            let mut attempted = Vec::new();
            let mode = try_capture_modes(&options, 30, |request| {
                attempted.push(request);
                if request == RequestedFormatType::HighestFrameRate(30) {
                    Ok(request)
                } else {
                    Err(anyhow!("Camera rejected mode"))
                }
            })
            .unwrap();
            let desired = CameraFormat::new(Resolution::new(640, 480), FrameFormat::YUYV, 30);
            let first = if cfg!(target_os = "macos") {
                RequestedFormatType::Closest(desired)
            } else {
                RequestedFormatType::Exact(desired)
            };
            assert_eq!(attempted.first(), Some(&first));
            assert_eq!(attempted.last(), Some(&mode));
        }

        /// Keeps a 4:3 capture centered on the 16:9 codec canvas.
        #[test]
        fn preserves_camera_aspect_ratio() {
            let input = RgbImage::from_pixel(640, 480, image::Rgb([47, 93, 94]));
            let frame = letterbox(Canvas::DEFAULT, input).unwrap();
            assert_eq!((frame.width(), frame.height()), (384, 216));
            assert_eq!(frame.get_pixel(47, 100).0, [0, 0, 0]);
            assert_eq!(frame.get_pixel(48, 100).0, [47, 93, 94]);
            assert_eq!(frame.get_pixel(335, 100).0, [47, 93, 94]);
            assert_eq!(frame.get_pixel(336, 100).0, [0, 0, 0]);
        }

        /// Checks packed YUYV chroma sharing and luma ordering through the native capture fast path.
        #[test]
        fn samples_packed_camera_pixels() {
            let pair = [35, 82, 193, 157];
            let bytes = pair.repeat(WIDTH * HEIGHT / 2);
            let frame = Buffer::new(
                Resolution::new(WIDTH as u32, HEIGHT as u32),
                &bytes,
                FrameFormat::YUYV,
            );
            let canvas = camera_canvas(&frame, Canvas::DEFAULT).unwrap();
            assert_eq!(canvas, frame.decode_image::<RgbFormat>().unwrap());
        }

        /// Direct sampling matches converting the complete camera frame before layered fitting.
        #[test]
        fn layered_camera_samples_match_full_rgb() -> Result<()> {
            let (width, height) = (1920, 1080);
            let bytes: Vec<_> =
                (0..width * height * 2).map(|i| (i * 157 + (i / 37) * 53 + 43) as u8).collect();
            let frame = Buffer::new(Resolution::new(width, height), &bytes, FrameFormat::YUYV);
            for canvas in [Canvas::new(1920, 1080)?, Canvas::new(320, 240)?] {
                let full = camera_canvas(&frame, canvas)?;
                for analysis in [layers::PLANE, layers::DETAIL_PLANE] {
                    let expected = layers::sample_grid(canvas, analysis, |x, y| {
                        full.get_pixel(x as u32, y as u32).0
                    });
                    assert_eq!(
                        layered_canvas(&frame, canvas, analysis)?.into_raw(),
                        expected.into_flattened()
                    );
                }
            }
            Ok(())
        }

        /// CPU and GPU sampling preserve RGB clipping and rounding, including a partial output word.
        #[tokio::test]
        async fn mesh_samples_match_full_size_yuyv_conversion() -> Result<()> {
            let source = Canvas { width: 10, height: 6 };
            let samples = crate::polygon::sample_canvas(source);
            let bytes: Vec<_> =
                (0..source.width * source.height * 2).map(|i| (i * 157 + 43) as u8).collect();
            let frame = Buffer::new(
                Resolution::new(source.width as u32, source.height as u32),
                &bytes,
                FrameFormat::YUYV,
            );
            let reference = letterbox(samples, camera_canvas(&frame, source)?)?;
            let packed = packed_mesh_input(&frame, samples).context("expected packed YUYV")?;
            assert_eq!(cpu_mesh_samples(packed, samples)?, reference);
            if let Some(gpu) = MeshSampler::new(samples).await? {
                for _ in 0..3 {
                    assert_eq!(gpu.sample(packed)?, reference);
                }
            }
            Ok(())
        }

        /// Averages all four pixels of a 2×2 source block into one output pixel.
        #[test]
        fn averages_a_downscaled_pixel() {
            let mut input = RgbImage::new((2 * WIDTH) as u32, (2 * HEIGHT) as u32);
            input.put_pixel(0, 0, image::Rgb([255, 0, 0]));
            input.put_pixel(1, 0, image::Rgb([0, 255, 0]));
            input.put_pixel(0, 1, image::Rgb([0, 0, 255]));
            let output = letterbox(Canvas::DEFAULT, input).unwrap();
            assert_eq!(output.get_pixel(0, 0).0, [64; 3]);
            assert_eq!(output.get_pixel(1, 0).0, [0; 3]);
        }
    }
}

pub(crate) use native::{list_cameras, run_worker};
