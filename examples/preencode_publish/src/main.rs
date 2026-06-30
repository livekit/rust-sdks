use std::{
    net::{Shutdown, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
#[cfg(feature = "gstreamer")]
use gstreamer as gst;
#[cfg(feature = "gstreamer")]
use gstreamer::prelude::*;
#[cfg(feature = "gstreamer")]
use gstreamer_app as gst_app;
use livekit::{prelude::*, webrtc::video_source::VideoResolution};
use livekit_api::access_token;
#[cfg(feature = "gstreamer")]
use livekit_capture::sources::gstreamer::{
    GStreamerAppSinkConfig, GStreamerAppSinkEncodedSource, GStreamerSampleFormat,
};
use livekit_capture::{
    encoded::h26x::annex_b_nal_ranges,
    sources::{
        rtsp::{RtspEncodedSource, RtspSourceOptions},
        tcp::{ByteStreamSourceConfig, TcpEncodedSource},
    },
    CaptureError, EncodedAccessUnitSource, EncodedFrameType, EncodedVideoCodec, EncodedWireFormat,
    OwnedEncodedAccessUnit, VideoCaptureTrack,
};

const DIAGNOSTIC_REPORT_INTERVAL: Duration = Duration::from_secs(1);
const SOURCE_STALL_THRESHOLD: Duration = Duration::from_millis(250);
const BURST_WALL_DELTA_THRESHOLD: Duration = Duration::from_millis(5);
const KEYFRAME_GAP_THRESHOLD: Duration = Duration::from_secs(5);
#[cfg(feature = "gstreamer")]
const GSTREAMER_APPSINK_NAME: &str = "lk_appsink";

/// Publish a pre-encoded video stream into a LiveKit room.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Encoded stream source.
    #[arg(long, value_enum, default_value_t = SourceKind::Tcp)]
    source: SourceKind,

    /// Encoded video codec. Required with --source tcp; optional validation with --source rtsp.
    /// Optional with --source gstappsink; omitted custom GStreamer pipelines infer H.264/H.265
    /// from their unlinked encoded output when possible.
    #[arg(long, value_enum)]
    codec: Option<CodecArg>,

    /// TCP server address as host:port. Required with --source tcp.
    #[arg(long)]
    host: Option<String>,

    /// RTSP URL. Required with --source rtsp.
    #[arg(long)]
    rtsp_url: Option<String>,

    /// LiveKit server URL.
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,

    /// LiveKit API key.
    #[arg(long, env = "LIVEKIT_API_KEY")]
    api_key: String,

    /// LiveKit API secret.
    #[arg(long, env = "LIVEKIT_API_SECRET")]
    api_secret: String,

    /// Room name to join.
    #[arg(long)]
    room_name: String,

    /// Participant identity to publish as.
    #[arg(long)]
    identity: String,

    /// Encoded frame width in pixels.
    #[arg(long, default_value_t = 1920)]
    width: u32,

    /// Encoded frame height in pixels.
    #[arg(long, default_value_t = 1080)]
    height: u32,

    /// Frame rate used for generated video and fallback timestamps.
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Log access-unit timing, keyframe, and H26x NAL diagnostics.
    #[arg(long)]
    diagnostics: bool,

    /// GStreamer launch pipeline used with --source gstappsink. If the pipeline does not include
    /// appsink name=lk_appsink, an H.264/H.265 parser and appsink are attached to its unlinked
    /// output.
    #[cfg(feature = "gstreamer")]
    #[arg(last = true, value_name = "PIPELINE")]
    gstreamer_pipeline: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SourceKind {
    Tcp,
    Rtsp,
    #[cfg(feature = "gstreamer")]
    Gstappsink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CodecArg {
    H264,
    H265,
}

impl CodecArg {
    fn encoded_codec(self) -> EncodedVideoCodec {
        match self {
            Self::H264 => EncodedVideoCodec::H264,
            Self::H265 => EncodedVideoCodec::H265,
        }
    }

    fn wire_format(self) -> EncodedWireFormat {
        match self {
            Self::H264 => EncodedWireFormat::H264AnnexB,
            Self::H265 => EncodedWireFormat::H265AnnexB,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    run(Args::parse()).await
}

async fn run(args: Args) -> Result<()> {
    validate_dimensions(args.width, args.height)?;
    #[cfg(feature = "gstreamer")]
    validate_gstreamer_args(&args)?;

    match args.source {
        SourceKind::Tcp => {
            let frame_interval_us = frame_interval_us(args.fps)?;
            run_tcp_source(args, frame_interval_us).await
        }
        SourceKind::Rtsp => run_rtsp_source(args).await,
        #[cfg(feature = "gstreamer")]
        SourceKind::Gstappsink => {
            let frame_interval_us = frame_interval_us(args.fps)?;
            run_gstreamer_source(args, frame_interval_us).await
        }
    }
}

#[cfg(feature = "gstreamer")]
fn validate_gstreamer_args(args: &Args) -> Result<()> {
    if args.source != SourceKind::Gstappsink && !args.gstreamer_pipeline.is_empty() {
        bail!("trailing GStreamer pipeline arguments are only valid with --source gstappsink");
    }
    Ok(())
}

async fn run_tcp_source(args: Args, frame_interval_us: i64) -> Result<()> {
    let codec_arg = args.codec.context("--codec is required with --source tcp")?;
    let codec = codec_arg.encoded_codec();
    let host = args.host.clone().context("--host is required with --source tcp")?;
    let config = ByteStreamSourceConfig::new(
        codec_arg.wire_format(),
        current_time_us(),
        frame_interval_us,
        args.width,
        args.height,
    );

    log::info!("Connecting to TCP encoded stream at {host}");
    let stream = TcpStream::connect(&host)
        .with_context(|| format!("failed to connect to TCP source at {host}"))?;
    let shutdown_stream = stream.try_clone().context("failed to clone TCP stream")?;
    let source = TcpEncodedSource::from_tcp_stream(stream, config)?;

    publish_encoded_source(
        args,
        codec,
        "TCP",
        source,
        move || {
            let _ = shutdown_stream.shutdown(Shutdown::Both);
        },
        Some(frame_interval_us),
    )
    .await
}

async fn run_rtsp_source(args: Args) -> Result<()> {
    let rtsp_url = args.rtsp_url.clone().context("--rtsp-url is required with --source rtsp")?;
    let mut options =
        RtspSourceOptions::new(args.width, args.height).with_start_timestamp_us(current_time_us());
    if let Some(codec) = args.codec {
        options = options.with_expected_codec(codec.encoded_codec());
    }

    log::info!("Connecting to RTSP encoded stream at {rtsp_url}");
    let source = RtspEncodedSource::connect(&rtsp_url, options)
        .with_context(|| format!("failed to connect to RTSP source at {rtsp_url}"))?;
    let shutdown_stream = source.try_clone_stream().context("failed to clone RTSP TCP stream")?;
    let codec = source.session_info().codec;
    log::info!(
        "RTSP setup selected {:?} payload type {} on interleaved channel {}",
        codec,
        source.session_info().payload_type,
        source.session_info().video_channel
    );

    publish_encoded_source(
        args,
        codec,
        "RTSP",
        source,
        move || {
            let _ = shutdown_stream.shutdown(Shutdown::Both);
        },
        None,
    )
    .await
}

#[cfg(feature = "gstreamer")]
async fn run_gstreamer_source(args: Args, frame_interval_us: i64) -> Result<()> {
    let source = GStreamerTestSource::start(
        args.width,
        args.height,
        args.fps,
        current_time_us(),
        frame_interval_us,
        args.codec.map(CodecArg::encoded_codec),
        &args.gstreamer_pipeline,
    )?;
    let codec = source.codec();
    let shutdown_pipeline = source.shutdown_pipeline();
    log::info!("Started GStreamer {:?} pipeline: {}", codec, source.pipeline_description());

    publish_encoded_source(
        args,
        codec,
        "GStreamer",
        source,
        move || {
            let _ = shutdown_pipeline.set_state(gst::State::Null);
        },
        Some(frame_interval_us),
    )
    .await
}

#[cfg(feature = "gstreamer")]
#[derive(Debug)]
struct GStreamerTestSource {
    pipeline: gst::Pipeline,
    source: GStreamerAppSinkEncodedSource,
    pipeline_description: String,
}

#[cfg(feature = "gstreamer")]
impl GStreamerTestSource {
    fn start(
        width: u32,
        height: u32,
        fps: u32,
        start_timestamp_us: i64,
        frame_interval_us: i64,
        requested_codec: Option<EncodedVideoCodec>,
        pipeline_args: &[String],
    ) -> Result<Self> {
        gst::init().context("failed to initialize GStreamer")?;

        let generated_codec = requested_codec.unwrap_or(EncodedVideoCodec::H264);
        let pipeline_description =
            gstreamer_pipeline_description(width, height, fps, generated_codec, pipeline_args);
        let element = gst::parse::launch(&pipeline_description).with_context(|| {
            format!("failed to create GStreamer pipeline: {pipeline_description}")
        })?;
        let Ok(pipeline) = element.downcast::<gst::Pipeline>() else {
            bail!("GStreamer description did not create a pipeline");
        };
        let requested_codec =
            if pipeline_args.is_empty() { Some(generated_codec) } else { requested_codec };
        let (appsink, sample_format) = ensure_encoded_appsink(&pipeline, requested_codec)?;
        let Ok(appsink) = appsink.downcast::<gst_app::AppSink>() else {
            bail!("GStreamer element {GSTREAMER_APPSINK_NAME} was not an appsink");
        };

        let config = GStreamerAppSinkConfig::new(
            sample_format,
            start_timestamp_us,
            frame_interval_us,
            width,
            height,
        );
        pipeline
            .set_state(gst::State::Playing)
            .context("failed to start GStreamer test pipeline")?;

        Ok(Self {
            pipeline,
            source: GStreamerAppSinkEncodedSource::new(appsink, config),
            pipeline_description,
        })
    }

    fn pipeline_description(&self) -> &str {
        &self.pipeline_description
    }

    fn codec(&self) -> EncodedVideoCodec {
        self.source.config().sample_format.codec()
    }

    fn shutdown_pipeline(&self) -> gst::Pipeline {
        self.pipeline.clone()
    }
}

#[cfg(feature = "gstreamer")]
impl EncodedAccessUnitSource for GStreamerTestSource {
    type Error = <GStreamerAppSinkEncodedSource as EncodedAccessUnitSource>::Error;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        self.source.next_access_unit()
    }
}

#[cfg(feature = "gstreamer")]
impl Drop for GStreamerTestSource {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(feature = "gstreamer")]
fn gstreamer_pipeline_description(
    width: u32,
    height: u32,
    fps: u32,
    codec: EncodedVideoCodec,
    pipeline_args: &[String],
) -> String {
    if pipeline_args.is_empty() {
        return gstreamer_test_pipeline_description(width, height, fps, codec);
    }

    pipeline_args.join(" ")
}

#[cfg(feature = "gstreamer")]
fn gstreamer_test_pipeline_description(
    width: u32,
    height: u32,
    fps: u32,
    codec: EncodedVideoCodec,
) -> String {
    let key_int_max = fps.max(1);
    let (encoder, parser, caps) = match codec {
        EncodedVideoCodec::H264 => (
            format!(
                "x264enc tune=zerolatency speed-preset=ultrafast key-int-max={key_int_max} \
                 bitrate=2500 byte-stream=true aud=true"
            ),
            "h264parse config-interval=-1",
            "video/x-h264,stream-format=byte-stream,alignment=au",
        ),
        EncodedVideoCodec::H265 => (
            format!(
                "x265enc tune=zerolatency speed-preset=ultrafast key-int-max={key_int_max} \
                 bitrate=2500"
            ),
            "h265parse config-interval=-1",
            "video/x-h265,stream-format=byte-stream,alignment=au",
        ),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            unreachable!("GStreamer generated test pipeline only supports H.264/H.265")
        }
        _ => unreachable!("unknown generated GStreamer codec"),
    };

    format!(
        "videotestsrc is-live=true do-timestamp=true pattern=smpte ! \
         video/x-raw,width={width},height={height},framerate={fps}/1 ! \
         timeoverlay halignment=right valignment=bottom shaded-background=true ! \
         videoconvert ! \
         {encoder} ! \
         {parser} ! \
         {caps} ! \
         appsink name={GSTREAMER_APPSINK_NAME} sync=false max-buffers=8 drop=true"
    )
}

#[cfg(feature = "gstreamer")]
fn ensure_encoded_appsink(
    pipeline: &gst::Pipeline,
    requested_codec: Option<EncodedVideoCodec>,
) -> Result<(gst::Element, GStreamerSampleFormat)> {
    if let Some(appsink) = pipeline.by_name(GSTREAMER_APPSINK_NAME) {
        let codec = requested_codec
            .or_else(|| codec_from_element_sink_caps(&appsink))
            .unwrap_or(EncodedVideoCodec::H264);
        let sample_format = h26x_sample_format(codec)?;
        return Ok((appsink, sample_format));
    }

    let src_pad = pipeline.find_unlinked_pad(gst::PadDirection::Src).with_context(|| {
        format!("GStreamer pipeline must include appsink name={GSTREAMER_APPSINK_NAME} or leave one H.264/H.265 source pad unlinked")
    })?;
    let inferred_codec = codec_from_pad_caps(&src_pad).with_context(|| {
        format!(
            "unlinked GStreamer pad '{}' does not advertise video/x-h264 or video/x-h265 caps",
            src_pad.name()
        )
    })?;
    let codec = match requested_codec {
        Some(requested_codec) if requested_codec != inferred_codec => bail!(
            "GStreamer codec mismatch: --codec requested {:?}, but unlinked pad '{}' advertises {:?}",
            requested_codec,
            src_pad.name(),
            inferred_codec
        ),
        Some(requested_codec) => requested_codec,
        None => inferred_codec,
    };
    let sample_format = h26x_sample_format(codec)?;
    let Some(src_element) = src_pad.parent_element() else {
        bail!("unlinked GStreamer encoded pad has no parent element");
    };

    let parser = gst::ElementFactory::make(h26x_parser_name(codec)?)
        .property("config-interval", -1i32)
        .build()
        .with_context(|| format!("failed to create {}", h26x_parser_name(codec).unwrap()))?;
    let codec_caps = h26x_appsink_caps(codec)?;
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", codec_caps)
        .build()
        .with_context(|| format!("failed to create {:?} capsfilter", codec))?;
    let appsink = gst::ElementFactory::make("appsink")
        .name(GSTREAMER_APPSINK_NAME)
        .property("sync", false)
        .property("max-buffers", 8u32)
        .property("drop", true)
        .build()
        .context("failed to create appsink")?;

    pipeline
        .add(&parser)
        .with_context(|| format!("failed to add {} to GStreamer pipeline", parser.name()))?;
    pipeline.add(&capsfilter).context("failed to add capsfilter to GStreamer pipeline")?;
    pipeline.add(&appsink).context("failed to add appsink to GStreamer pipeline")?;
    gst::Element::link_many([&parser, &capsfilter, &appsink])
        .with_context(|| format!("failed to link {} to appsink", parser.name()))?;
    let sink_pad = parser
        .static_pad("sink")
        .with_context(|| format!("{} did not expose a sink pad", parser.name()))?;
    src_pad
        .link(&sink_pad)
        .with_context(|| format!("failed to link '{}' to {}", src_element.name(), parser.name()))?;

    Ok((appsink, sample_format))
}

#[cfg(feature = "gstreamer")]
fn h26x_sample_format(codec: EncodedVideoCodec) -> Result<GStreamerSampleFormat> {
    match codec {
        EncodedVideoCodec::H264 => Ok(GStreamerSampleFormat::H264AnnexB),
        EncodedVideoCodec::H265 => Ok(GStreamerSampleFormat::H265AnnexB),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => bail!(
            "GStreamer passthrough currently supports H.264/H.265 Annex-B; {:?} needs an explicit access-unit source path",
            codec
        ),
        _ => bail!("unsupported GStreamer codec: {:?}", codec),
    }
}

#[cfg(feature = "gstreamer")]
fn h26x_parser_name(codec: EncodedVideoCodec) -> Result<&'static str> {
    match codec {
        EncodedVideoCodec::H264 => Ok("h264parse"),
        EncodedVideoCodec::H265 => Ok("h265parse"),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            bail!("no H26x parser for {:?}", codec)
        }
        _ => bail!("unsupported GStreamer codec: {:?}", codec),
    }
}

#[cfg(feature = "gstreamer")]
fn h26x_caps_name(codec: EncodedVideoCodec) -> Result<&'static str> {
    match codec {
        EncodedVideoCodec::H264 => Ok("video/x-h264"),
        EncodedVideoCodec::H265 => Ok("video/x-h265"),
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
            bail!("GStreamer passthrough currently supports H.264/H.265 Annex-B")
        }
        _ => bail!("unsupported GStreamer codec: {:?}", codec),
    }
}

#[cfg(feature = "gstreamer")]
fn h26x_appsink_caps(codec: EncodedVideoCodec) -> Result<gst::Caps> {
    Ok(gst::Caps::builder(h26x_caps_name(codec)?)
        .field("stream-format", "byte-stream")
        .field("alignment", "au")
        .build())
}

#[cfg(feature = "gstreamer")]
fn codec_from_element_sink_caps(element: &gst::Element) -> Option<EncodedVideoCodec> {
    let sink_pad = element.static_pad("sink")?;
    codec_from_pad_caps(&sink_pad)
}

#[cfg(feature = "gstreamer")]
fn codec_from_pad_caps(pad: &gst::Pad) -> Option<EncodedVideoCodec> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.iter().find_map(|structure| codec_from_caps_name(structure.name()))
}

#[cfg(feature = "gstreamer")]
fn codec_from_caps_name(name: &str) -> Option<EncodedVideoCodec> {
    match name {
        "video/x-h264" => Some(EncodedVideoCodec::H264),
        "video/x-h265" => Some(EncodedVideoCodec::H265),
        _ => None,
    }
}

async fn publish_encoded_source<S, ShutdownSource>(
    args: Args,
    codec: EncodedVideoCodec,
    source_label: &'static str,
    source: S,
    shutdown_source: ShutdownSource,
    expected_frame_interval_us: Option<i64>,
) -> Result<()>
where
    S: EncodedAccessUnitSource + Send + 'static,
    ShutdownSource: FnOnce() + Send + 'static,
{
    let diagnostics_enabled = args.diagnostics;
    let token = access_token::AccessToken::with_api_key(&args.api_key, &args.api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            can_publish: true,
            can_subscribe: false,
            ..Default::default()
        })
        .to_jwt()?;

    log::info!("Connecting to LiveKit room '{}' as '{}'", args.room_name, args.identity);
    let (room, _) = Room::connect(&args.url, &token, RoomOptions::default())
        .await
        .context("failed to connect to LiveKit room")?;

    let capture_track = VideoCaptureTrack::new(
        "preencoded",
        VideoResolution { width: args.width, height: args.height },
        false,
    );
    let mut publish_options = VideoCaptureTrack::encoded_publish_options(codec);
    publish_options.source = TrackSource::Camera;

    room.local_participant()
        .publish_track(LocalTrack::Video(capture_track.track()), publish_options)
        .await
        .context("failed to publish pre-encoded video track")?;
    log::info!(
        "Published pre-encoded {:?} track at {}x{}; forwarding {} access units",
        codec,
        args.width,
        args.height,
        source_label
    );

    let stop = Arc::new(AtomicBool::new(false));
    let signal_task = tokio::spawn({
        let stop = stop.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            stop.store(true, Ordering::Release);
            shutdown_source();
        }
    });

    let capture_task = tokio::task::spawn_blocking({
        let stop = stop.clone();
        move || {
            let diagnostics = AccessUnitDiagnostics::new(
                diagnostics_enabled,
                source_label,
                expected_frame_interval_us,
            );
            forward_access_units(source, capture_track, stop, diagnostics)
        }
    });
    let captured = capture_task.await.context("capture task failed to join")??;
    signal_task.abort();
    room.close().await.context("failed to close LiveKit room")?;

    log::info!("Stopped after publishing {captured} encoded access units");
    Ok(())
}

fn forward_access_units<S>(
    mut source: S,
    track: VideoCaptureTrack,
    stop: Arc<AtomicBool>,
    mut diagnostics: AccessUnitDiagnostics,
) -> Result<u64>
where
    S: EncodedAccessUnitSource,
{
    let mut captured = 0;
    let mut dropped = 0;
    while !stop.load(Ordering::Acquire) {
        let read_started = Instant::now();
        let access_unit = match source.next_access_unit() {
            Ok(Some(access_unit)) => access_unit,
            Ok(None) => break,
            Err(err) if stop.load(Ordering::Acquire) => {
                log::debug!("encoded source stopped after shutdown: {err}");
                break;
            }
            Err(err) => return Err(err.into()),
        };
        diagnostics.observe_source_wait(read_started.elapsed());
        diagnostics.observe_access_unit(&access_unit);

        match track.capture_encoded(&access_unit.as_access_unit()) {
            Ok(()) => {}
            Err(CaptureError::CaptureFailed) => {
                dropped += 1;
                if dropped == 1 || dropped % 300 == 0 {
                    log::info!("Dropped {dropped} encoded access units before capture");
                }
                continue;
            }
            Err(err) => return Err(err.into()),
        }
        captured += 1;
        if captured % 300 == 0 {
            log::info!("Published {captured} encoded access units");
        }
    }
    diagnostics.finish();

    Ok(captured)
}

#[derive(Debug)]
struct AccessUnitDiagnostics {
    enabled: bool,
    source_label: &'static str,
    expected_frame_interval_us: Option<i64>,
    last_report: Instant,
    last_wall_time: Option<Instant>,
    last_timestamp_us: Option<i64>,
    last_keyframe_wall_time: Option<Instant>,
    last_keyframe_warning: Option<Instant>,
    total_frames: u64,
    total_keyframes: u64,
    report_frames: u64,
    report_keyframes: u64,
    report_bytes: u64,
    report_max_bytes: usize,
    report_max_source_wait: Duration,
    report_max_wall_gap: Duration,
    report_max_timestamp_gap_us: i64,
    report_stalls: u64,
    report_bursts: u64,
    report_missing_parameter_keyframes: u64,
}

impl AccessUnitDiagnostics {
    fn new(
        enabled: bool,
        source_label: &'static str,
        expected_frame_interval_us: Option<i64>,
    ) -> Self {
        let now = Instant::now();
        if enabled {
            match expected_frame_interval_us {
                Some(interval_us) => log::info!(
                    "{source_label} diagnostics enabled; expected frame interval {:.2}ms",
                    interval_us as f64 / 1000.0
                ),
                None => log::info!("{source_label} diagnostics enabled"),
            }
        }

        Self {
            enabled,
            source_label,
            expected_frame_interval_us,
            last_report: now,
            last_wall_time: None,
            last_timestamp_us: None,
            last_keyframe_wall_time: None,
            last_keyframe_warning: None,
            total_frames: 0,
            total_keyframes: 0,
            report_frames: 0,
            report_keyframes: 0,
            report_bytes: 0,
            report_max_bytes: 0,
            report_max_source_wait: Duration::ZERO,
            report_max_wall_gap: Duration::ZERO,
            report_max_timestamp_gap_us: 0,
            report_stalls: 0,
            report_bursts: 0,
            report_missing_parameter_keyframes: 0,
        }
    }

    fn observe_source_wait(&mut self, wait: Duration) {
        if !self.enabled {
            return;
        }

        self.report_max_source_wait = self.report_max_source_wait.max(wait);
        if wait > SOURCE_STALL_THRESHOLD {
            self.report_stalls += 1;
            log::warn!(
                "{} source wait {:.1}ms before next access unit",
                self.source_label,
                wait.as_secs_f64() * 1000.0
            );
        }
    }

    fn observe_access_unit(&mut self, access_unit: &OwnedEncodedAccessUnit) {
        if !self.enabled {
            return;
        }

        let now = Instant::now();
        let payload = access_unit.payload.as_ref();
        let payload_len = payload.len();
        let nal_summary = NalSummary::from_annex_b(access_unit.codec, payload);
        let is_keyframe = access_unit.frame_type == EncodedFrameType::Key;
        let timestamp_gap_us =
            self.last_timestamp_us.map(|last| access_unit.timestamp_us.saturating_sub(last));

        self.total_frames += 1;
        self.report_frames += 1;
        self.report_bytes = self.report_bytes.saturating_add(payload_len as u64);
        self.report_max_bytes = self.report_max_bytes.max(payload_len);
        if is_keyframe {
            self.total_keyframes += 1;
            self.report_keyframes += 1;
            self.last_keyframe_wall_time = Some(now);
            self.last_keyframe_warning = None;
        }

        if let Some(last_wall_time) = self.last_wall_time {
            let wall_gap = now.saturating_duration_since(last_wall_time);
            self.report_max_wall_gap = self.report_max_wall_gap.max(wall_gap);
            if wall_gap > SOURCE_STALL_THRESHOLD {
                self.report_stalls += 1;
                log::warn!(
                    "{} publish gap {:.1}ms before frame {}",
                    self.source_label,
                    wall_gap.as_secs_f64() * 1000.0,
                    self.total_frames
                );
            }
            if wall_gap < BURST_WALL_DELTA_THRESHOLD {
                if let Some(timestamp_gap_us) = timestamp_gap_us {
                    if timestamp_gap_us > BURST_WALL_DELTA_THRESHOLD.as_micros() as i64 {
                        self.report_bursts += 1;
                    }
                }
            }
        }

        if let Some(timestamp_gap_us) = timestamp_gap_us {
            self.report_max_timestamp_gap_us =
                self.report_max_timestamp_gap_us.max(timestamp_gap_us);
            self.observe_timestamp_gap(timestamp_gap_us);
        }

        if is_keyframe {
            if nal_summary.missing_recovery_parameter_set() {
                self.report_missing_parameter_keyframes += 1;
                log::warn!(
                    "{} keyframe {} missing recovery parameter sets: {}",
                    self.source_label,
                    self.total_frames,
                    nal_summary.describe(access_unit.codec)
                );
            } else {
                log::info!(
                    "{} keyframe {} ts={} size={} {}",
                    self.source_label,
                    self.total_frames,
                    access_unit.timestamp_us,
                    payload_len,
                    nal_summary.describe(access_unit.codec)
                );
            }
        } else if nal_summary.contains_key_picture {
            log::warn!(
                "{} access unit {} contains a key picture but is marked delta: {}",
                self.source_label,
                self.total_frames,
                nal_summary.describe(access_unit.codec)
            );
        }

        self.warn_if_keyframe_gap(now);
        self.last_wall_time = Some(now);
        self.last_timestamp_us = Some(access_unit.timestamp_us);
        self.report_if_due(now);
    }

    fn observe_timestamp_gap(&mut self, timestamp_gap_us: i64) {
        let Some(expected_us) = self.expected_frame_interval_us else {
            return;
        };
        let tolerance_us = (expected_us / 2).max(10_000);
        let deviation_us = (timestamp_gap_us - expected_us).abs();
        if deviation_us > tolerance_us {
            log::warn!(
                "{} timestamp gap {:.2}ms differs from expected {:.2}ms",
                self.source_label,
                timestamp_gap_us as f64 / 1000.0,
                expected_us as f64 / 1000.0
            );
        }
    }

    fn warn_if_keyframe_gap(&mut self, now: Instant) {
        let Some(last_keyframe_wall_time) = self.last_keyframe_wall_time else {
            if self.total_frames > 1
                && self.last_keyframe_warning.is_none_or(|last| {
                    now.saturating_duration_since(last) >= KEYFRAME_GAP_THRESHOLD
                })
            {
                self.last_keyframe_warning = Some(now);
                log::warn!(
                    "{} has not seen a keyframe after {} access units",
                    self.source_label,
                    self.total_frames
                );
            }
            return;
        };

        let keyframe_gap = now.saturating_duration_since(last_keyframe_wall_time);
        if keyframe_gap >= KEYFRAME_GAP_THRESHOLD
            && self
                .last_keyframe_warning
                .is_none_or(|last| now.saturating_duration_since(last) >= KEYFRAME_GAP_THRESHOLD)
        {
            self.last_keyframe_warning = Some(now);
            log::warn!(
                "{} no keyframe for {:.1}s; passthrough cannot satisfy PLI without upstream IDR",
                self.source_label,
                keyframe_gap.as_secs_f64()
            );
        }
    }

    fn report_if_due(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last_report);
        if elapsed < DIAGNOSTIC_REPORT_INTERVAL {
            return;
        }

        let avg_size =
            if self.report_frames == 0 { 0 } else { self.report_bytes / self.report_frames };
        let fps = self.report_frames as f64 / elapsed.as_secs_f64();
        log::info!(
            "{} diagnostics: frames={} fps={:.1} keys={} avg_size={} max_size={} \
             max_source_wait={:.1}ms max_publish_gap={:.1}ms max_ts_gap={:.1}ms stalls={} \
             bursts={} missing_param_keys={}",
            self.source_label,
            self.report_frames,
            fps,
            self.report_keyframes,
            avg_size,
            self.report_max_bytes,
            self.report_max_source_wait.as_secs_f64() * 1000.0,
            self.report_max_wall_gap.as_secs_f64() * 1000.0,
            self.report_max_timestamp_gap_us as f64 / 1000.0,
            self.report_stalls,
            self.report_bursts,
            self.report_missing_parameter_keyframes
        );
        self.reset_report(now);
    }

    fn reset_report(&mut self, now: Instant) {
        self.last_report = now;
        self.report_frames = 0;
        self.report_keyframes = 0;
        self.report_bytes = 0;
        self.report_max_bytes = 0;
        self.report_max_source_wait = Duration::ZERO;
        self.report_max_wall_gap = Duration::ZERO;
        self.report_max_timestamp_gap_us = 0;
        self.report_stalls = 0;
        self.report_bursts = 0;
        self.report_missing_parameter_keyframes = 0;
    }

    fn finish(&mut self) {
        if !self.enabled {
            return;
        }

        log::info!(
            "{} diagnostics finished: frames={} keyframes={}",
            self.source_label,
            self.total_frames,
            self.total_keyframes
        );
    }
}

#[derive(Debug, Default)]
struct NalSummary {
    nal_count: usize,
    vcl_count: usize,
    aud_count: usize,
    vps_count: usize,
    sps_count: usize,
    pps_count: usize,
    contains_key_picture: bool,
}

impl NalSummary {
    fn from_annex_b(codec: EncodedVideoCodec, payload: &[u8]) -> Self {
        let mut summary = Self::default();
        for range in annex_b_nal_ranges(payload) {
            let nal = &payload[range];
            if nal.is_empty() {
                continue;
            }

            match codec {
                EncodedVideoCodec::H264 => summary.observe_h264(nal[0] & 0x1f),
                EncodedVideoCodec::H265 => {
                    if nal.len() >= 2 {
                        summary.observe_h265((nal[0] >> 1) & 0x3f);
                    }
                }
                EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {}
                _ => {}
            }
        }
        summary
    }

    fn observe_h264(&mut self, nal_type: u8) {
        self.nal_count += 1;
        if (1..=5).contains(&nal_type) {
            self.vcl_count += 1;
        }
        match nal_type {
            5 => self.contains_key_picture = true,
            7 => self.sps_count += 1,
            8 => self.pps_count += 1,
            9 => self.aud_count += 1,
            _ => {}
        }
    }

    fn observe_h265(&mut self, nal_type: u8) {
        self.nal_count += 1;
        if nal_type <= 31 {
            self.vcl_count += 1;
        }
        match nal_type {
            16..=21 => self.contains_key_picture = true,
            32 => self.vps_count += 1,
            33 => self.sps_count += 1,
            34 => self.pps_count += 1,
            35 => self.aud_count += 1,
            _ => {}
        }
    }

    fn missing_recovery_parameter_set(&self) -> bool {
        self.sps_count == 0 || self.pps_count == 0
    }

    fn describe(&self, codec: EncodedVideoCodec) -> String {
        match codec {
            EncodedVideoCodec::H264 => format!(
                "nals={} vcl={} aud={} sps={} pps={} key_picture={}",
                self.nal_count,
                self.vcl_count,
                self.aud_count,
                self.sps_count,
                self.pps_count,
                self.contains_key_picture
            ),
            EncodedVideoCodec::H265 => format!(
                "nals={} vcl={} aud={} vps={} sps={} pps={} key_picture={}",
                self.nal_count,
                self.vcl_count,
                self.aud_count,
                self.vps_count,
                self.sps_count,
                self.pps_count,
                self.contains_key_picture
            ),
            EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 | EncodedVideoCodec::AV1 => {
                "non-H26x payload".to_string()
            }
            _ => "unknown encoded payload".to_string(),
        }
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        bail!("--width and --height must be greater than zero");
    }
    Ok(())
}

fn frame_interval_us(fps: u32) -> Result<i64> {
    if fps == 0 {
        bail!("--fps must be greater than zero");
    }
    Ok(1_000_000_i64 / i64::from(fps))
}

fn current_time_us() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    duration.as_micros().min(i64::MAX as u128) as i64
}

#[cfg(all(test, feature = "gstreamer"))]
mod tests {
    use super::*;

    #[test]
    fn gstreamer_pipeline_description_routes_test_source_to_h264_appsink() {
        let description =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::H264);

        assert!(description.contains("videotestsrc is-live=true do-timestamp=true"));
        assert!(description.contains("timeoverlay"));
        assert!(description.contains("x264enc"));
        assert!(description.contains("video/x-h264,stream-format=byte-stream,alignment=au"));
        assert!(description.contains(&format!("appsink name={GSTREAMER_APPSINK_NAME}")));
    }

    #[test]
    fn gstreamer_pipeline_description_routes_test_source_to_h265_appsink() {
        let description =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::H265);

        assert!(description.contains("videotestsrc is-live=true do-timestamp=true"));
        assert!(description.contains("timeoverlay"));
        assert!(description.contains("x265enc"));
        assert!(description.contains("h265parse config-interval=-1"));
        assert!(description.contains("video/x-h265,stream-format=byte-stream,alignment=au"));
        assert!(description.contains(&format!("appsink name={GSTREAMER_APPSINK_NAME}")));
    }

    #[test]
    fn gstreamer_pipeline_description_uses_trailing_pipeline_args() {
        let pipeline = [
            "videotestsrc".to_string(),
            "is-live=true".to_string(),
            "!".to_string(),
            "x264enc".to_string(),
        ];

        assert_eq!(
            gstreamer_pipeline_description(320, 180, 30, EncodedVideoCodec::H265, &pipeline),
            "videotestsrc is-live=true ! x264enc"
        );
    }

    #[test]
    fn gstreamer_test_source_pulls_h264_access_units_when_plugins_are_available() {
        let frame_interval_us = frame_interval_us(30).unwrap();
        let mut source = match GStreamerTestSource::start(
            320,
            180,
            30,
            10_000,
            frame_interval_us,
            Some(EncodedVideoCodec::H264),
            &[],
        ) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("skipping GStreamer appsink smoke test: {err:#}");
                return;
            }
        };

        assert_h264_access_units(&mut source);
    }

    #[test]
    fn gstreamer_test_source_pulls_h265_access_units_when_plugins_are_available() {
        let frame_interval_us = frame_interval_us(30).unwrap();
        let mut source = match GStreamerTestSource::start(
            320,
            180,
            30,
            10_000,
            frame_interval_us,
            Some(EncodedVideoCodec::H265),
            &[],
        ) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("skipping GStreamer H.265 appsink smoke test: {err:#}");
                return;
            }
        };

        assert_h265_access_units(&mut source);
    }

    #[test]
    fn gstreamer_test_source_attaches_appsink_to_trailing_h264_pipeline() {
        let frame_interval_us = frame_interval_us(30).unwrap();
        let pipeline = [
            "videotestsrc".to_string(),
            "is-live=true".to_string(),
            "do-timestamp=true".to_string(),
            "pattern=smpte".to_string(),
            "!".to_string(),
            "video/x-raw,width=320,height=180,framerate=30/1".to_string(),
            "!".to_string(),
            "videoconvert".to_string(),
            "!".to_string(),
            "x264enc".to_string(),
            "tune=zerolatency".to_string(),
            "speed-preset=ultrafast".to_string(),
            "key-int-max=30".to_string(),
            "byte-stream=true".to_string(),
            "aud=true".to_string(),
        ];
        let mut source = match GStreamerTestSource::start(
            320,
            180,
            30,
            10_000,
            frame_interval_us,
            None,
            &pipeline,
        ) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("skipping custom GStreamer pipeline smoke test: {err:#}");
                return;
            }
        };

        assert_h264_access_units(&mut source);
    }

    #[test]
    fn gstreamer_test_source_attaches_appsink_to_trailing_h265_pipeline() {
        let frame_interval_us = frame_interval_us(30).unwrap();
        let pipeline = [
            "videotestsrc".to_string(),
            "is-live=true".to_string(),
            "do-timestamp=true".to_string(),
            "pattern=smpte".to_string(),
            "!".to_string(),
            "video/x-raw,width=320,height=180,framerate=30/1".to_string(),
            "!".to_string(),
            "videoconvert".to_string(),
            "!".to_string(),
            "x265enc".to_string(),
            "tune=zerolatency".to_string(),
            "speed-preset=ultrafast".to_string(),
            "key-int-max=30".to_string(),
            "bitrate=2500".to_string(),
        ];
        let mut source = match GStreamerTestSource::start(
            320,
            180,
            30,
            10_000,
            frame_interval_us,
            None,
            &pipeline,
        ) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("skipping custom GStreamer H.265 pipeline smoke test: {err:#}");
                return;
            }
        };

        assert_h265_access_units(&mut source);
    }

    fn assert_h264_access_units(source: &mut GStreamerTestSource) {
        let first = source
            .next_access_unit()
            .expect("GStreamer appsink source should read the first sample")
            .expect("GStreamer appsink should produce a first access unit");
        let second = source
            .next_access_unit()
            .expect("GStreamer appsink source should read the second sample")
            .expect("GStreamer appsink should produce a second access unit");

        assert_eq!(first.codec, EncodedVideoCodec::H264);
        assert_eq!(first.width, 320);
        assert_eq!(first.height, 180);
        assert!(!first.payload.is_empty());
        assert!(first.timestamp_us >= 10_000);
        assert!(second.timestamp_us > first.timestamp_us);
    }

    fn assert_h265_access_units(source: &mut GStreamerTestSource) {
        let first = source
            .next_access_unit()
            .expect("GStreamer appsink source should read the first sample")
            .expect("GStreamer appsink should produce a first access unit");
        let second = source
            .next_access_unit()
            .expect("GStreamer appsink source should read the second sample")
            .expect("GStreamer appsink should produce a second access unit");

        assert_eq!(first.codec, EncodedVideoCodec::H265);
        assert_eq!(first.width, 320);
        assert_eq!(first.height, 180);
        assert!(!first.payload.is_empty());
        assert!(first.timestamp_us >= 10_000);
        assert!(second.timestamp_us > first.timestamp_us);
    }
}
