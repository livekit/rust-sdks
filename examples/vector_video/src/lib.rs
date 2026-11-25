mod benchmark;
mod capture;
mod codec;
mod compression;
mod layers;
mod messages;
mod palette;
mod polygon;
mod timing;
mod transport;
mod video;
mod viewer;

use std::{collections::VecDeque, path::PathBuf, time::Duration};

use anyhow::{ensure, Result};
use clap::{Args, Parser, Subcommand};

use crate::{
    codec::{Budget, DecodedFrame, Encoder, FrameRate, WireFormat, DEFAULT_OVERHEAD},
    transport::ConnectionOptions,
    video::{Canvas, Sink, Source},
};

const TRACK_NAME: &str = "vector-video-v1";

/// Streams vector video over a LiveKit Data Track.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Mode,
}

/// Selects local comparison, sender, or receiver mode.
#[derive(Subcommand)]
enum Mode {
    /// Save source, reconstruction, and outline alignment as a three-panel MP4.
    Preview {
        #[command(flatten)]
        input: FileInput,
        output: PathBuf,
        /// Lossless wire compression to apply before reconstructing the preview.
        #[arg(
            long,
            value_enum,
            default_value = "predictive",
            default_value_if(
                "low_bandwidth",
                clap::builder::ArgPredicate::IsPresent,
                "mesh-patch"
            )
        )]
        compression: compression::Compression,
        /// Simulate losing every Nth Data Track message.
        #[arg(long, value_parser = clap::value_parser!(u64).range(2..))]
        drop_every: Option<u64>,
    },
    /// Compare wire compression methods on identical frames and verify exact reconstruction.
    Benchmark(FileInput),
    /// Capture and publish vector video on a LiveKit Data Track.
    Publish(Publisher),
    /// Reconstruct and display vector video from a LiveKit Data Track.
    Subscribe(Subscriber),
}

/// Camera or file input and the settings for publishing its frames.
#[derive(Parser)]
#[command(about = "Capture and publish vector video on a LiveKit Data Track")]
struct Publisher {
    #[command(flatten)]
    connection: ConnectionOptions,
    /// Write per-frame timing events to a new CSV file. Use different files on each peer.
    #[arg(long)]
    latency_csv: Option<PathBuf>,
    /// Read a video file or URL instead of using the local camera.
    #[arg(long, conflicts_with_all = ["camera_index", "list_cameras"])]
    input: Option<String>,
    /// FFmpeg input format. Requires --input and a known frame rate.
    #[arg(long, requires = "input")]
    input_format: Option<String>,
    /// Override the camera or file cadence (default 15 fps).
    #[arg(long, default_value_t = FrameRate::POLYGON_DEFAULT)]
    fps: FrameRate,
    /// Local camera index. Use --list-cameras to list devices.
    #[arg(long, default_value_t = 0)]
    camera_index: u32,
    /// List available local cameras and exit.
    #[arg(long)]
    list_cameras: bool,
    /// Run the isolated camera reader used by the publisher.
    #[arg(long, hide = true)]
    capture_worker: bool,
    #[command(flatten)]
    rate: RateOptions,
    /// Lossless wire compression. Use off for older subscribers using preset grids.
    #[arg(
        long,
        value_enum,
        default_value = "predictive",
        default_value_if("low_bandwidth", clap::builder::ArgPredicate::IsPresent, "mesh-patch")
    )]
    compression: compression::Compression,
    /// Name of the LiveKit Data Track to publish.
    #[arg(long, default_value = TRACK_NAME)]
    track: String,
}

/// Selects one Data Track and controls where the reconstructed frames go.
#[derive(Parser)]
#[command(about = "Reconstruct and display vector video from a LiveKit Data Track")]
struct Subscriber {
    #[command(flatten)]
    connection: ConnectionOptions,
    /// Write per-frame timing events to a new CSV file. Use different files on each peer.
    #[arg(long)]
    latency_csv: Option<PathBuf>,
    /// Name of the LiveKit Data Track to subscribe to.
    #[arg(long, default_value = TRACK_NAME)]
    track: String,
    /// Subscribe only to this participant identity.
    #[arg(long)]
    publisher: Option<String>,
    /// Limit the native viewer's refresh rate, selecting the newest frame at each refresh.
    #[arg(long, conflicts_with="output", value_parser=clap::value_parser!(u16).range(1..=240))]
    max_display_fps: Option<u16>,
    /// Save a reconstructed MP4 instead of opening the live viewer.
    #[arg(long)]
    output: Option<PathBuf>,
}

/// File or FFmpeg device input used by the offline comparison.
#[derive(Args)]
struct FileInput {
    /// File, URL, or FFmpeg capture device.
    source: String,
    /// Constant FPS, e.g. 30 or 30000/1001. Defaults to 15.
    #[arg(long, default_value_t = FrameRate::POLYGON_DEFAULT)]
    fps: FrameRate,
    /// FFmpeg input format. Uses the selected mode cadence or --fps.
    #[arg(long)]
    input_format: Option<String>,
    #[command(flatten)]
    rate: RateOptions,
}

/// Shared sender and preview rate configuration.
#[derive(Args)]
struct RateOptions {
    /// Video width in pixels. With only one dimension specified, infer the other at 16:9.
    #[arg(long)]
    width: Option<u16>,
    /// Video height in pixels. Use --height 720 for 1280×720.
    #[arg(long)]
    height: Option<u16>,
    /// Maximum visible triangles per frame. Selects colored-polygon mode (default cap 432).
    #[arg(long, value_parser = clap::value_parser!(u32).range(2..))]
    max_triangles: Option<u32>,
    /// Maximum overlapping shapes, excluding the background.
    #[arg(
        long,
        value_parser = clap::value_parser!(u16).range(1..=i64::from(layers::MAX_LAYERS)),
        conflicts_with = "max_triangles",
        help = format!("Maximum overlapping shapes, excluding the background (default {}).", layers::DEFAULT_LAYERS)
    )]
    max_layers: Option<u16>,
    /// Prioritize small structural features in layered polygons.
    #[arg(long)]
    feature_layers: bool,
    /// Use the multi-scale contour search in layered polygons.
    #[arg(long)]
    detailed_layers: bool,
    /// Represent thin linear features as centerline strokes in layered polygons.
    #[arg(long)]
    line_strokes: bool,
    /// Reserve layers for detected faces and facial landmarks on the publisher CPU.
    #[arg(long)]
    face_layers: bool,
    /// Guide foreground outlines with a segmentation model (requires the object-layers feature).
    #[arg(long)]
    object_layers: bool,
    /// Select the foreground model used by --object-layers. Defaults to onnx.
    #[arg(long, value_enum, requires = "object_layers")]
    object_provider: Option<layers::ObjectProvider>,
    /// Load an external ONNX foreground model and run it on the CPU.
    #[arg(long, requires = "object_layers")]
    object_model: Option<PathBuf>,
    /// Guide small detail polygons with a contextual ONNX model (requires the object-layers feature).
    #[arg(long, requires = "feature_layers", conflicts_with = "face_layers")]
    part_model: Option<PathBuf>,
    /// Bytes reserved per message for transport overhead.
    #[arg(long, default_value_t = DEFAULT_OVERHEAD)]
    overhead_bytes: usize,
    /// Select the polygon representation.
    #[arg(
        long = "mode",
        visible_alias = "wire-format",
        value_enum,
        default_value = "layered-polygon",
        default_value_if(
            "max_triangles",
            clap::builder::ArgPredicate::IsPresent,
            "colored-polygon"
        ),
        default_value_if(
            "low_bandwidth",
            clap::builder::ArgPredicate::IsPresent,
            "colored-polygon"
        )
    )]
    wire_format: WireFormat,
    /// Colored polygon refinement priority (default: features).
    #[arg(long, value_enum)]
    detail_priority: Option<polygon::DetailPriority>,
    /// Share one color across connected low-contrast triangles in colored-polygon mode.
    #[arg(long)]
    merge_flat_regions: bool,
    /// Select colored polygons with sparse color updates and mesh-patch compression by default.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "sparse",
        value_name = "sparse", value_parser = parse_low_bandwidth)]
    low_bandwidth: Option<()>,
}

/// Accepts the historical qualitative spellings and explains migration from the capped profile.
fn parse_low_bandwidth(value: &str) -> Result<(), &'static str> {
    match value {
        "sparse" | "low" | "minimum" => Ok(()),
        "10kbps" => {
            Err("the 10 kbps cap was removed. Use --low-bandwidth for a variable-rate sparse mesh")
        }
        _ => Err("use --low-bandwidth to enable the sparse mesh"),
    }
}

impl RateOptions {
    /// Applies the reference policy shared by the publisher, preview and benchmark.
    fn packet_encoder(&self, mode: compression::Compression) -> compression::PacketEncoder {
        let packer = compression::PacketEncoder::new(mode);
        if self.low_bandwidth.is_some() {
            packer.with_longer_mesh_recovery()
        } else {
            packer
        }
    }

    /// Resolves the requested output dimensions, using even pixels for MP4 chroma sampling.
    fn canvas(&self) -> Result<Canvas> {
        let even = |n: u16, a: u32, b: u32| -> Result<u16> {
            Ok(u16::try_from(((u32::from(n) * a + b) / (2 * b)).max(1) * 2)?)
        };
        match (self.width, self.height) {
            (Some(w), Some(h)) => Canvas::new(w, h),
            (Some(w), None) => Canvas::new(w, even(w, 9, 16)?),
            (None, Some(h)) => Canvas::new(even(h, 16, 9)?, h),
            (None, None) => Ok(Canvas::DEFAULT),
        }
    }

    /// Creates a sender with the selected refinement priority and model guidance.
    fn encoder(&self, requested: layers::InferenceMode) -> Result<Encoder> {
        ensure!(
            self.object_model.is_none()
                || self.object_provider != Some(layers::ObjectProvider::AppleVision),
            "--object-model requires --object-provider onnx"
        );
        let guidance = layers::Guidance {
            features: self.feature_layers,
            budgeted_search: !self.detailed_layers && self.part_model.is_none(),
            line_strokes: self.line_strokes,
            faces: if self.face_layers { requested } else { layers::InferenceMode::Disabled },
            #[cfg(feature = "object-layers")]
            objects: if self.object_layers { requested } else { layers::InferenceMode::Disabled },
            #[cfg(feature = "object-layers")]
            object_provider: self.object_provider.unwrap_or_default(),
            #[cfg(feature = "object-layers")]
            object_model: self.object_model.clone(),
            #[cfg(feature = "object-layers")]
            parts: self.part_model.as_ref().map(|path| (path.clone(), requested)),
        };
        Encoder::new(
            polygon::Options {
                priority: self.detail_priority.unwrap_or_default(),
                merge_flat_regions: self.merge_flat_regions,
                low_bandwidth: self.low_bandwidth.is_some(),
            },
            guidance,
        )
    }

    /// Derives the size bound from the selected polygon mode.
    fn budget(&self, fps: FrameRate) -> Result<Budget> {
        let canvas = self.canvas()?;
        let budget = match self.wire_format {
            WireFormat::ColoredPolygon => {
                ensure!(self.max_layers.is_none(), "--max-layers requires --mode layered-polygon");
                ensure!(!self.feature_layers, "--feature-layers requires --mode layered-polygon");
                ensure!(!self.detailed_layers, "--detailed-layers requires --mode layered-polygon");
                ensure!(!self.line_strokes, "--line-strokes requires --mode layered-polygon");
                ensure!(!self.face_layers, "--face-layers requires --mode layered-polygon");
                ensure!(!self.object_layers, "--object-layers requires --mode layered-polygon");
                let plan = polygon::MeshPlan::for_triangles(
                    self.max_triangles.unwrap_or(polygon::DEFAULT_TRIANGLES),
                    canvas,
                    self.detail_priority.unwrap_or_default(),
                    self.low_bandwidth.is_some(),
                );
                eprintln!("Mesh capacity: {} visible triangles/frame", plan.max_triangles());
                Budget::for_triangles(plan, canvas, self.overhead_bytes)
            }
            WireFormat::LayeredPolygon => {
                ensure!(
                    self.low_bandwidth.is_none(),
                    "--low-bandwidth requires --mode colored-polygon"
                );
                ensure!(
                    !self.merge_flat_regions,
                    "--merge-flat-regions requires --mode colored-polygon"
                );
                if self.part_model.is_some() {
                    ensure!(
                        cfg!(feature = "object-layers"),
                        "--part-model requires a build with --features object-layers"
                    );
                    ensure!(
                        canvas.width >= layers::DETAIL_PLANE.width
                            && canvas.height >= layers::DETAIL_PLANE.height,
                        "--part-model requires a canvas of at least 384×216"
                    );
                }
                ensure!(
                    !self.object_layers || cfg!(feature = "object-layers"),
                    "--object-layers requires a build with --features object-layers"
                );
                ensure!(
                    self.object_provider != Some(layers::ObjectProvider::AppleVision)
                        || cfg!(all(target_os = "macos", feature = "apple-vision")),
                    "--object-provider apple-vision requires macOS and --features apple-vision"
                );
                ensure!(
                    self.max_triangles.is_none(),
                    "--max-triangles requires --mode colored-polygon"
                );
                ensure!(
                    self.detail_priority.is_none(),
                    "--detail-priority requires --mode colored-polygon"
                );
                Budget::for_layers(
                    self.max_layers.unwrap_or(layers::DEFAULT_LAYERS),
                    canvas,
                    self.overhead_bytes,
                )
            }
        };
        eprintln!(
            "Video {}×{} at {fps} fps, {} maximum snapshot bytes, {} byte/message overhead allowance",
            canvas.width, canvas.height, budget.payload, budget.overhead
        );
        Ok(budget)
    }
}

/// Runs the combined example with preview, publish, and subscribe subcommands.
pub async fn run() -> Result<()> {
    match Cli::parse().command {
        Mode::Preview { input, output, compression, drop_every } => {
            preview(&input, &output, compression, drop_every).await
        }
        Mode::Benchmark(input) => benchmark::run(&input).await,
        Mode::Publish(args) => publish(args, &["publish"]).await,
        Mode::Subscribe(args) => subscribe(args).await,
    }
}

/// Runs the standalone vector video publisher.
pub async fn run_publisher() -> Result<()> {
    publish(Publisher::parse(), &[]).await
}

/// Runs the standalone vector video subscriber.
pub async fn run_subscriber() -> Result<()> {
    subscribe(Subscriber::parse()).await
}

/// Opens the selected source, then hands its negotiated frame rate to the codec and sender.
async fn publish(args: Publisher, worker_prefix: &[&str]) -> Result<()> {
    if args.list_cameras {
        return capture::list_cameras();
    }
    let canvas = args.rate.canvas()?;
    if args.capture_worker {
        return tokio::task::block_in_place(|| {
            capture::run_worker(&args.camera_options(canvas), args.fps)
        });
    }
    transport::publish(&args, worker_prefix).await
}

impl Publisher {
    /// Requests the stream dimensions from the camera, preserving the default 640×480 device request.
    fn camera_options(&self, canvas: Canvas) -> capture::CameraOptions {
        capture::CameraOptions {
            camera_index: self.camera_index,
            requested: (self.rate.width.is_some() || self.rate.height.is_some()).then_some(canvas),
            mode: self.rate.wire_format,
            face_layers: self.rate.face_layers,
            feature_layers: self.rate.feature_layers,
        }
    }

    /// Resolves a camera, file, URL, or explicitly formatted FFmpeg input.
    async fn open(&self, worker_prefix: &[&str]) -> Result<Option<(FrameRate, Source)>> {
        let canvas = self.rate.canvas()?;
        if let Some(input) = &self.input {
            let source = Source::file(input, self.input_format.as_deref(), self.fps, canvas)?;
            return Ok(Some((self.fps, source)));
        }
        capture::open(&self.camera_options(canvas), self.fps, worker_prefix).await
    }
}

/// Starts LiveKit subscription and playback using the selected sink.
async fn subscribe(args: Subscriber) -> Result<()> {
    let trace = timing::Trace::open(args.latency_csv.as_deref())?;
    let result = if let Some(output) = &args.output {
        transport::subscribe(&args, transport::Output::File(output), &trace, std::future::pending())
            .await
    } else {
        viewer::run(args, trace.clone()).await
    };
    result.and(trace.finish())
}

/// Renders the same serialized payloads the publisher would send, with optional message loss.
async fn preview(
    input: &FileInput,
    output: &std::path::Path,
    compression: compression::Compression,
    drop_every: Option<u64>,
) -> Result<()> {
    let fps = input.fps;
    let budget = input.rate.budget(fps)?;
    let canvas = budget.canvas;
    let mut encoder = input.rate.encoder(layers::InferenceMode::Offline)?;
    let mut source = Source::file(&input.source, input.input_format.as_deref(), fps, canvas)?;
    let mut sink = match Sink::open(output, fps, canvas, true) {
        Ok(sink) => sink,
        Err(error) => {
            source.stop().await?;
            return Err(error);
        }
    };
    let mut meter = Meter::new(budget.overhead);
    let mut frame_count = 0;
    let mut shape_count = 0;
    let mut dropped = 0;
    let mut last_batch = None::<DecodedFrame>;
    let mut packer = input.rate.packet_encoder(compression);
    let mut decoder = messages::Receiver::default();
    let result = async {
        while let Some(frame) = source.read_frame(budget.format()).await? {
            let mut packet = encoder.encode(frame_count as u32, fps, &budget, &frame);
            packer.stabilize_outline_order(&mut packet);
            let packet = packer.pack(packet)?;
            let mut next = None;
            for message in messages::split(frame_count as u32, packet) {
                meter.record(fps.duration(frame_count), message.len());
                if drop_every.is_some_and(|n| meter.messages.is_multiple_of(n)) {
                    dropped += 1;
                } else if let Some(batch) = decoder.decode(&message)? {
                    next = Some(batch);
                }
            }
            if next.is_some() {
                last_batch = next;
            }
            if let Some(batch) = &last_batch {
                let (picture, shapes) = batch.comparison(&frame.rgb);
                shape_count += shapes as u64;
                sink.write(&picture).await?;
            }
            frame_count += 1;
        }
        ensure!(frame_count > 0, "Input contained no video frames");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let stopped = source.stop().await;
    let finished = sink.finish().await;
    result.and(stopped).and(finished)?;
    meter.report(frame_count, fps.duration(frame_count));
    let shapes = match budget.format() {
        WireFormat::LayeredPolygon => "painted polygons",
        WireFormat::ColoredPolygon => "visible triangles",
    };
    eprintln!("Average {:.1} {shapes} per frame", shape_count as f64 / frame_count as f64);
    eprintln!("Simulated dropped messages: {dropped}");
    Ok(())
}

/// Counts payload bytes and modeled overhead over rolling one-second windows.
struct Meter {
    overhead: usize,
    messages: u64,
    bytes: u64,
    recent: VecDeque<(Duration, usize)>,
    recent_bytes: usize,
    peak_bytes: usize,
    peak_payload_bytes: usize,
}

impl Meter {
    /// Starts an empty meter with the specified allowance per message.
    fn new(overhead: usize) -> Self {
        Self {
            overhead,
            messages: 0,
            bytes: 0,
            recent: VecDeque::new(),
            recent_bytes: 0,
            peak_bytes: 0,
            peak_payload_bytes: 0,
        }
    }

    /// Records an attempted or simulated send at a monotonic timestamp.
    fn record(&mut self, stamp: Duration, payload: usize) {
        self.messages += 1;
        self.bytes += payload as u64;
        let bytes = payload + self.overhead;
        while self.recent.front().is_some_and(|&(time, _)| stamp - time >= Duration::from_secs(1)) {
            if let Some((_, bytes)) = self.recent.pop_front() {
                self.recent_bytes -= bytes;
            }
        }
        self.recent.push_back((stamp, bytes));
        self.recent_bytes += bytes;
        self.peak_bytes = self.peak_bytes.max(self.recent_bytes);
        self.peak_payload_bytes =
            self.peak_payload_bytes.max(self.recent_bytes - self.recent.len() * self.overhead);
    }

    /// Reports application payload and modeled transport rates for the run.
    fn report(&self, frames: u64, elapsed: Duration) {
        let seconds = elapsed.as_secs_f64();
        if seconds > 0.0 {
            let accounted = self.bytes + self.messages * self.overhead as u64;
            eprintln!("{frames} source frames / {} messages / {seconds:.2}s ({:.1} fps)\nPayload avg {:.0} bps / accounted avg {:.0} bps / accounted peak (1s) {} bps", self.messages, frames as f64 / seconds, self.bytes as f64 * 8.0 / seconds, accounted as f64 * 8.0 / seconds, self.peak_bytes * 8);
            eprintln!("Peak payload (1s) {} bps", self.peak_payload_bytes * 8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File and camera publishers share the canvas flags and retain the default device request.
    #[test]
    fn publisher_resolves_requested_dimensions() {
        let hd = Canvas::new(1280, 720).unwrap();
        for args in [
            &["--height", "720"][..],
            &["--width", "1280"],
            &["--input", "clip.mp4", "--width", "1280", "--height", "720"],
        ] {
            let publisher =
                Publisher::try_parse_from(["publish"].into_iter().chain(args.iter().copied()))
                    .unwrap();
            assert_eq!(publisher.rate.canvas().unwrap(), hd);
            assert_eq!(publisher.camera_options(hd).requested, Some(hd));
        }
        let publisher = Publisher::try_parse_from(["publish"]).unwrap();
        assert_eq!(publisher.rate.canvas().unwrap(), Canvas::DEFAULT);
        assert!(publisher.camera_options(Canvas::DEFAULT).requested.is_none());
        for args in [["--width", "0"], ["--height", "721"], ["--width", "8192"]] {
            assert!(Publisher::try_parse_from(["publish"].into_iter().chain(args))
                .unwrap()
                .rate
                .canvas()
                .is_err());
        }
    }

    /// The visible mode choices and encoded versions agree for both entry points.
    #[test]
    fn supports_only_the_two_polygon_modes() -> Result<()> {
        let defaults = Publisher::try_parse_from(["publish"])?;
        assert!(defaults.rate.wire_format == WireFormat::LayeredPolygon);
        assert_eq!(defaults.fps, FrameRate::POLYGON_DEFAULT);
        assert_eq!(
            defaults.rate.budget(defaults.fps)?.payload,
            codec::header_len(7) + layers::Scene::body_bytes(48)
        );
        let mesh = Publisher::try_parse_from(["publish", "--max-triangles", "500", "--fps", "30"])?;
        assert!(mesh.rate.wire_format == WireFormat::ColoredPolygon);
        assert_eq!(mesh.fps, FrameRate::CAMERA_DEFAULT);
        for (option, name, version) in
            [("--mode", "layered-polygon", 7), ("--wire-format", "colored-polygon", 6)]
        {
            let args = Publisher::try_parse_from(["publish", option, name])?;
            let budget = args.rate.budget(args.fps)?;
            let frame = video::AnalyzedFrame::new(vec![255; video::WIDTH * video::HEIGHT * 3]);
            let mut data = Encoder::default().encode(17, args.fps, &budget, &frame);
            assert_eq!(data[2], version);
            assert!(data.len() <= budget.payload);
            let batch = DecodedFrame::decode(&data)?;
            assert_eq!((batch.start, batch.fps), (17, args.fps));
            assert!(batch.render().iter().all(|&v| v == 255));
            data[11] = 2;
            assert!(DecodedFrame::decode(&data).is_err());
            data[11] = 1;
            for retired in [1, 2, 3, 4, 5, 8, 9, 10] {
                data[2] = retired;
                assert!(DecodedFrame::decode(&data).is_err());
            }
        }
        for name in ["contours", "v1", "v2", "v3", "v4", "v5", "curved-shape", "colored-quads"] {
            assert!(Publisher::try_parse_from(["publish", "--mode", name]).is_err());
            assert!(Cli::try_parse_from(["vector_video", "benchmark", "input", "--mode", name])
                .is_err());
        }
        Ok(())
    }

    /// All qualitative spellings use one sparse profile in the publisher, preview and benchmark.
    #[test]
    fn selects_sparse_mesh_and_its_wire_default() -> Result<()> {
        use compression::Compression;
        for flag in [
            "--low-bandwidth",
            "--low-bandwidth=sparse",
            "--low-bandwidth=low",
            "--low-bandwidth=minimum",
        ] {
            let publisher = Publisher::try_parse_from(["publish", flag, "--max-triangles", "500"])?;
            assert!(publisher.rate.low_bandwidth.is_some());
            assert!(publisher.compression == Compression::MeshPatch);
            let budget = publisher.rate.budget(publisher.fps)?;
            let preview = Cli::try_parse_from([
                "vector_video",
                "preview",
                flag,
                "in",
                "out",
                "--max-triangles",
                "500",
            ])?;
            let Mode::Preview { input, compression, .. } = preview.command else { unreachable!() };
            assert!(compression == Compression::MeshPatch);
            assert_eq!(input.rate.budget(input.fps)?.payload, budget.payload);
            let benchmark = Cli::try_parse_from([
                "vector_video",
                "benchmark",
                flag,
                "in",
                "--max-triangles",
                "500",
            ])?;
            let Mode::Benchmark(input) = benchmark.command else { unreachable!() };
            assert!(input.rate.low_bandwidth.is_some());
        }
        let standard = Publisher::try_parse_from(["publish", "--max-triangles", "500"])?;
        assert!(standard.rate.low_bandwidth.is_none());
        assert!(standard.compression == Compression::Predictive);
        let sparse = Publisher::try_parse_from(["publish", "--low-bandwidth"])?;
        assert!(sparse.rate.budget(sparse.fps)?.format() == WireFormat::ColoredPolygon);
        let explicit = Publisher::try_parse_from([
            "publish",
            "--low-bandwidth",
            "--compression",
            "compact",
            "--max-triangles",
            "500",
        ])?;
        assert!(explicit.compression == Compression::Compact);
        let layered =
            Publisher::try_parse_from(["publish", "--mode", "layered-polygon", "--low-bandwidth"])?;
        assert!(layered.rate.budget(layered.fps).is_err());
        let capped =
            Publisher::try_parse_from(["publish", "--low-bandwidth=10kbps"]).err().unwrap();
        assert!(capped.to_string().contains("10 kbps cap was removed"));
        assert!(Publisher::try_parse_from(["publish", "--low-bandwidth=unknown"]).is_err());
        Ok(())
    }

    /// Shape limits still select their polygon family and conflict across families.
    #[test]
    fn validates_polygon_options() -> Result<()> {
        let layers = Publisher::try_parse_from(["publish", "--max-layers", "24", "--face-layers"])?;
        assert!(layers.rate.wire_format == WireFormat::LayeredPolygon);
        assert_eq!(
            layers.rate.budget(layers.fps)?.payload,
            codec::header_len(7) + layers::Scene::body_bytes(24)
        );
        assert!(Publisher::try_parse_from(["publish", "--max-layers", "0"]).is_err());
        assert!(Publisher::try_parse_from(["publish", "--max-triangles", "1"]).is_err());
        for flags in [
            ["--mode", "layered-polygon", "--max-triangles", "24"],
            ["--mode", "colored-polygon", "--max-layers", "24"],
        ] {
            let args = Publisher::try_parse_from(["publish"].into_iter().chain(flags))?;
            assert!(args.rate.budget(args.fps).is_err());
        }
        let args =
            Publisher::try_parse_from(["publish", "--mode", "colored-polygon", "--face-layers"])?;
        assert!(args.rate.budget(args.fps).is_err());
        let args =
            Publisher::try_parse_from(["publish", "--feature-layers", "--max-layers", "48"])?;
        assert!(args.rate.budget(args.fps).is_ok());
        let args = Publisher::try_parse_from([
            "publish",
            "--mode",
            "colored-polygon",
            "--feature-layers",
        ])?;
        assert!(args.rate.budget(args.fps).is_err());
        Ok(())
    }

    /// Validates contextual guidance before the publisher opens its model.
    #[test]
    fn validates_part_model_options() -> Result<()> {
        assert!(Publisher::try_parse_from(["publish", "--part-model", "parts.onnx"]).is_err());
        assert!(Publisher::try_parse_from([
            "publish",
            "--feature-layers",
            "--face-layers",
            "--part-model",
            "parts.onnx",
        ])
        .is_err());

        let args = Publisher::try_parse_from([
            "publish",
            "--feature-layers",
            "--part-model",
            "parts.onnx",
            "--width",
            "192",
        ])?;
        assert!(args.rate.budget(args.fps).is_err());
        let args = Publisher::try_parse_from([
            "publish",
            "--feature-layers",
            "--part-model",
            "parts.onnx",
        ])?;
        assert_eq!(args.rate.budget(args.fps).is_ok(), cfg!(feature = "object-layers"));
        Ok(())
    }

    /// Allows extra foreground providers only when their build and command options support them.
    #[test]
    fn validates_foreground_provider_options() -> Result<()> {
        assert!(
            Publisher::try_parse_from(["publish", "--object-provider", "apple-vision"]).is_err()
        );
        let args = Publisher::try_parse_from([
            "publish",
            "--object-layers",
            "--object-provider",
            "apple-vision",
        ])?;
        assert_eq!(
            args.rate.budget(args.fps).is_ok(),
            cfg!(all(target_os = "macos", feature = "apple-vision"))
        );
        Ok(())
    }

    /// The rolling meter evicts messages once they are a second old.
    #[test]
    fn rolls_the_rate_window() {
        let mut meter = Meter::new(DEFAULT_OVERHEAD);
        for time in [0, 196, 397, 602, 801, 1000] {
            meter.record(Duration::from_millis(time), 256);
        }
        assert_eq!(meter.peak_bytes, 5 * (256 + DEFAULT_OVERHEAD));
        assert_eq!(meter.peak_payload_bytes, 5 * 256);
        assert_eq!(meter.messages, 6);
    }
}
