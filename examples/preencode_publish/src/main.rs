//! Publish a pre-encoded video stream into a LiveKit room.
//!
//! Encoded access units are pulled from a TCP, RTSP, or GStreamer source and
//! pumped into a passthrough `VideoCaptureTrack` by
//! `livekit_capture::EncodedIngress`, which also forwards downstream keyframe
//! requests (PLI/FIR from the SFU) back to the source. The higher-level
//! `livekit_capture::VideoCaptureSource` facade covers the same encoded
//! endpoints via `CaptureSourceOptions::encoded`; this example drives
//! `EncodedIngress` directly to keep its per-access-unit diagnostics.

use std::{
    net::{Shutdown, TcpStream},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
#[cfg(feature = "gstreamer")]
use gstreamer as gst;
#[cfg(feature = "gstreamer")]
use gstreamer::glib::{self, types::StaticType};
#[cfg(feature = "gstreamer")]
use gstreamer::prelude::*;
use livekit::{
    options::{self, FrameMetadataFeatures, VideoEncoding},
    prelude::*,
    webrtc::{video_frame::FrameMetadata, video_source::VideoResolution},
};
use livekit_api::access_token;
#[cfg(feature = "gstreamer")]
use livekit_capture::sources::gstreamer::{
    encoded_caps_string, ensure_encoded_appsink, GStreamerAppSinkConfig,
    GStreamerAppSinkEncodedSource, GStreamerBitrateUnit, GStreamerEncoderRateControl,
    ENCODED_APPSINK_NAME,
};
use livekit_capture::{
    sources::{
        rtsp::{RtspEncodedSource, RtspSourceOptions},
        tcp::{ByteStreamSourceConfig, TcpEncodedSource},
    },
    CaptureError, EncodedAccessUnitSource, EncodedFrameType, EncodedIngress, EncodedIngressCapture,
    EncodedIngressError, EncodedIngressStop, EncodedRateControl, EncodedVideoCodec,
    EncodedWireFormat, OwnedEncodedAccessUnit, VideoCaptureTrack,
};

const DIAGNOSTIC_REPORT_INTERVAL: Duration = Duration::from_secs(1);
const SOURCE_STALL_THRESHOLD: Duration = Duration::from_millis(250);
const BURST_WALL_DELTA_THRESHOLD: Duration = Duration::from_millis(5);
const MAX_PUBLISH_PACE_SLEEP: Duration = Duration::from_millis(100);
const PUBLISH_PACE_SLEEP_SLICE: Duration = Duration::from_millis(10);
const MIN_KEYFRAME_REQUEST_INTERVAL: Duration = Duration::from_secs(2);
const KEYFRAME_GAP_THRESHOLD: Duration = Duration::from_secs(5);
const H265_PREENCODED_BITRATE_HEADROOM: u64 = 2;
const AV1_PREENCODED_BITRATE_HEADROOM: u64 = 3;
#[cfg(feature = "gstreamer")]
const GSTREAMER_ENCODER_NAME: &str = "lk_encoder";
#[cfg(feature = "gstreamer")]
const GSTREAMER_BITRATE_OVERLAY_NAME: &str = "lk_bitrate_overlay";

/// Publish a pre-encoded video stream into a LiveKit room.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Encoded stream source.
    #[arg(long, value_enum, default_value_t = SourceKind::Tcpsink)]
    source: SourceKind,

    /// Encoded video codec. Required with --source tcpsink and --source shmsink; optional
    /// validation with --source rtsp. Optional with --source gstappsink; omitted custom
    /// GStreamer pipelines infer the codec from their unlinked encoded output when possible.
    #[arg(long, value_enum)]
    codec: Option<CodecArg>,

    /// TCP server address as host:port. Required with --source tcpsink.
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

    /// Maximum publish bitrate in bits per second. Generated GStreamer test
    /// sources use the same target bitrate so local smoke tests do not overrun
    /// the advertised send cap.
    #[arg(long)]
    max_bitrate: Option<u64>,

    /// H.264 TCP byte-stream format.
    #[arg(long, value_enum, default_value_t = H264FormatArg::AnnexB)]
    h264_format: H264FormatArg,

    /// Length-prefix size in bytes for --h264-format avc.
    #[arg(long, default_value_t = 4)]
    avc_nal_length_size: u8,

    /// TCP transport framing.
    #[arg(long, value_enum, default_value_t = TcpFormatArg::Auto)]
    tcp_format: TcpFormatArg,

    /// RTP timestamp clock rate used with --tcp-format rtp.
    #[arg(long, default_value_t = 90_000)]
    rtp_clock_rate: u32,

    /// Log access-unit timing, keyframe, and keyframe-request diagnostics.
    #[arg(long)]
    diagnostics: bool,

    /// Attach a wall-clock timestamp to each published encoded frame.
    #[arg(long)]
    attach_timestamp: bool,

    /// Attach a monotonically increasing frame id to each published encoded frame.
    #[arg(long)]
    attach_frame_id: bool,

    /// GStreamer shmsink socket path. Used with --source shmsink.
    #[cfg(feature = "gstreamer")]
    #[arg(long, default_value = "/tmp/livekit-preencode-test.shm")]
    shmsink_socket_path: String,

    /// Overlay the WebRTC target bitrate on generated GStreamer test video.
    #[cfg(feature = "gstreamer")]
    #[arg(long)]
    overlay_bitrate: bool,

    /// GStreamer launch pipeline used with --source gstappsink. If the pipeline does not include
    /// appsink name=lk_appsink, codec-specific normalization and an appsink are attached to its
    /// unlinked output. Name encoder element lk_encoder and textoverlay lk_bitrate_overlay to
    /// receive WebRTC bitrate updates.
    #[cfg(feature = "gstreamer")]
    #[arg(last = true, value_name = "PIPELINE")]
    gstreamer_pipeline: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SourceKind {
    Tcpsink,
    Rtsp,
    #[cfg(feature = "gstreamer")]
    Gstappsink,
    #[cfg(feature = "gstreamer")]
    Shmsink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CodecArg {
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum H264FormatArg {
    AnnexB,
    Avc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum TcpFormatArg {
    Auto,
    ByteStream,
    Rtp,
}

impl CodecArg {
    fn encoded_codec(self) -> EncodedVideoCodec {
        match self {
            Self::H264 => EncodedVideoCodec::H264,
            Self::H265 => EncodedVideoCodec::H265,
            Self::Vp8 => EncodedVideoCodec::VP8,
            Self::Vp9 => EncodedVideoCodec::VP9,
            Self::Av1 => EncodedVideoCodec::AV1,
        }
    }

    fn tcp_wire_format(
        self,
        tcp_format: TcpFormatArg,
        h264_format: H264FormatArg,
        avc_nal_length_size: u8,
        rtp_clock_rate: u32,
    ) -> Result<EncodedWireFormat> {
        match tcp_format.resolve(self) {
            ResolvedTcpFormat::ByteStream => match self {
                Self::H264 => match h264_format {
                    H264FormatArg::AnnexB => Ok(EncodedWireFormat::H264AnnexB),
                    H264FormatArg::Avc => {
                        Ok(EncodedWireFormat::H264Avc { nal_length_size: avc_nal_length_size })
                    }
                }
                Self::H265 => Ok(EncodedWireFormat::H265AnnexB),
                Self::Vp8 | Self::Vp9 | Self::Av1 => bail!(
                    "--tcp-format byte-stream is only supported for H.264/H.265; use --tcp-format rtp for {:?}",
                    self.encoded_codec()
                ),
            },
            ResolvedTcpFormat::Rtp => Ok(EncodedWireFormat::Rtp {
                codec: self.encoded_codec(),
                clock_rate: rtp_clock_rate,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedTcpFormat {
    ByteStream,
    Rtp,
}

impl TcpFormatArg {
    fn resolve(self, codec: CodecArg) -> ResolvedTcpFormat {
        match self {
            Self::Auto => match codec {
                CodecArg::H264 | CodecArg::H265 => ResolvedTcpFormat::ByteStream,
                CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1 => ResolvedTcpFormat::Rtp,
            },
            Self::ByteStream => ResolvedTcpFormat::ByteStream,
            Self::Rtp => ResolvedTcpFormat::Rtp,
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
    validate_max_bitrate(args.max_bitrate)?;
    validate_h264_format_args(&args)?;
    #[cfg(feature = "gstreamer")]
    validate_gstreamer_args(&args)?;

    match args.source {
        SourceKind::Tcpsink => {
            let frame_interval_us = frame_interval_us(args.fps)?;
            run_tcp_source(args, frame_interval_us).await
        }
        SourceKind::Rtsp => run_rtsp_source(args).await,
        #[cfg(feature = "gstreamer")]
        SourceKind::Gstappsink => {
            let frame_interval_us = frame_interval_us(args.fps)?;
            run_gstreamer_source(args, frame_interval_us).await
        }
        #[cfg(feature = "gstreamer")]
        SourceKind::Shmsink => {
            let frame_interval_us = frame_interval_us(args.fps)?;
            run_shmsink_source(args, frame_interval_us).await
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

fn validate_h264_format_args(args: &Args) -> Result<()> {
    if !(1..=4).contains(&args.avc_nal_length_size) {
        bail!("--avc-nal-length-size must be between 1 and 4 bytes");
    }
    if args.rtp_clock_rate == 0 {
        bail!("--rtp-clock-rate must be greater than zero");
    }
    if args.source == SourceKind::Tcpsink {
        if let Some(codec) = args.codec {
            if args.tcp_format.resolve(codec) == ResolvedTcpFormat::ByteStream
                && matches!(codec, CodecArg::Vp8 | CodecArg::Vp9 | CodecArg::Av1)
            {
                bail!("--tcp-format byte-stream is only supported for H.264/H.265");
            }
        }
    }
    if args.h264_format == H264FormatArg::Avc {
        if args.source != SourceKind::Tcpsink {
            bail!("--h264-format avc is only valid with --source tcpsink");
        }
        if args.tcp_format == TcpFormatArg::Rtp {
            bail!("--h264-format avc is only valid with TCP byte-stream input");
        }
        if args.codec != Some(CodecArg::H264) {
            bail!("--h264-format avc requires --codec h264");
        }
    }
    Ok(())
}

async fn run_tcp_source(args: Args, frame_interval_us: i64) -> Result<()> {
    let codec_arg = args.codec.context("--codec is required with --source tcpsink")?;
    let codec = codec_arg.encoded_codec();
    let host = args.host.clone().context("--host is required with --source tcpsink")?;
    let wire_format = codec_arg.tcp_wire_format(
        args.tcp_format,
        args.h264_format,
        args.avc_nal_length_size,
        args.rtp_clock_rate,
    )?;
    let config = ByteStreamSourceConfig::new(
        wire_format,
        current_time_us(),
        frame_interval_us,
        args.width,
        args.height,
    );

    log::info!("Connecting to TCP {wire_format:?} encoded stream at {host}");
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
    let overlay_bitrate = args.overlay_bitrate;
    let source = GStreamerTestSource::start(
        args.width,
        args.height,
        args.fps,
        current_time_us(),
        frame_interval_us,
        args.codec.map(CodecArg::encoded_codec),
        &args.gstreamer_pipeline,
        args.max_bitrate,
        overlay_bitrate,
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
async fn run_shmsink_source(args: Args, frame_interval_us: i64) -> Result<()> {
    let codec_arg = args.codec.context("--codec is required with --source shmsink")?;
    let codec = codec_arg.encoded_codec();
    let socket_path = args.shmsink_socket_path.clone();
    let pipeline_args = vec![gstreamer_shmsink_pipeline_description(&socket_path, codec)];
    let source = GStreamerTestSource::start(
        args.width,
        args.height,
        args.fps,
        current_time_us(),
        frame_interval_us,
        Some(codec),
        &pipeline_args,
        args.max_bitrate,
        false,
    )?;
    let shutdown_pipeline = source.shutdown_pipeline();
    log::info!(
        "Started GStreamer {:?} shmsink reader for {}: {}",
        codec,
        socket_path,
        source.pipeline_description()
    );

    publish_encoded_source(
        args,
        codec,
        "GStreamer shmsink",
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
    bitrate_overlay: Option<gst::Element>,
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
        max_bitrate: Option<u64>,
        overlay_bitrate: bool,
    ) -> Result<Self> {
        gst::init().context("failed to initialize GStreamer")?;

        let generated_codec = requested_codec.unwrap_or(EncodedVideoCodec::H264);
        let pipeline_description = gstreamer_pipeline_description(
            width,
            height,
            fps,
            generated_codec,
            pipeline_args,
            max_bitrate,
            overlay_bitrate,
        );
        let element = gst::parse::launch(&pipeline_description).with_context(|| {
            format!("failed to create GStreamer pipeline: {pipeline_description}")
        })?;
        let Ok(pipeline) = element.downcast::<gst::Pipeline>() else {
            bail!("GStreamer description did not create a pipeline");
        };
        let requested_codec =
            if pipeline_args.is_empty() { Some(generated_codec) } else { requested_codec };
        let (appsink, sample_format) = ensure_encoded_appsink(&pipeline, requested_codec)
            .context("failed to prepare GStreamer encoded appsink")?;
        let bitrate_overlay = pipeline.by_name(GSTREAMER_BITRATE_OVERLAY_NAME);

        let config = GStreamerAppSinkConfig::new(
            sample_format,
            start_timestamp_us,
            frame_interval_us,
            width,
            height,
        );
        let mut source = GStreamerAppSinkEncodedSource::new(appsink, config);
        if let Some(rate_control) = gstreamer_encoder_rate_control(&pipeline, sample_format.codec())
        {
            source.set_encoder_rate_control(rate_control);
        }
        pipeline
            .set_state(gst::State::Playing)
            .context("failed to start GStreamer test pipeline")?;

        Ok(Self { pipeline, source, bitrate_overlay, pipeline_description })
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

    fn request_keyframe(&mut self) {
        // Forward downstream PLI/FIR to the appsink source, which raises a
        // GstForceKeyUnit event so the upstream encoder emits an IDR.
        self.source.request_keyframe();
    }

    fn update_rate_control(&mut self, rate_control: EncodedRateControl) {
        if let Some(overlay) = &self.bitrate_overlay {
            update_bitrate_overlay(overlay, rate_control);
        }
        self.source.update_rate_control(rate_control);
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
    max_bitrate: Option<u64>,
    overlay_bitrate: bool,
) -> String {
    if pipeline_args.is_empty() {
        return gstreamer_test_pipeline_description(
            width,
            height,
            fps,
            codec,
            max_bitrate,
            overlay_bitrate,
        );
    }

    pipeline_args.join(" ")
}

#[cfg(feature = "gstreamer")]
fn gstreamer_test_pipeline_description(
    width: u32,
    height: u32,
    fps: u32,
    codec: EncodedVideoCodec,
    max_bitrate: Option<u64>,
    overlay_bitrate: bool,
) -> String {
    let bitrate = publish_video_encoding(max_bitrate, width, height, fps, codec).max_bitrate;
    let codec_pipeline = gstreamer_test_encode_pipeline(fps, codec, bitrate);
    let bitrate_overlay = if overlay_bitrate {
        format!(
            "textoverlay name={GSTREAMER_BITRATE_OVERLAY_NAME} text={} \
             halignment=left valignment=top shaded-background=true font-desc={} ! ",
            gstreamer_launch_string_value("WebRTC target: pending"),
            gstreamer_launch_string_value("Sans, 24"),
        )
    } else {
        String::new()
    };

    format!(
        "videotestsrc is-live=true do-timestamp=true pattern=ball motion=wavy animation-mode=frames ! \
         video/x-raw,width={width},height={height},framerate={fps}/1 ! \
         timeoverlay halignment=right valignment=bottom shaded-background=true ! \
         {bitrate_overlay}\
         videoconvert ! \
         video/x-raw,format=I420 ! \
         {codec_pipeline} ! \
         appsink name={ENCODED_APPSINK_NAME} sync=false max-buffers=8 drop=true"
    )
}

#[cfg(feature = "gstreamer")]
fn gstreamer_encoder_rate_control(
    pipeline: &gst::Pipeline,
    codec: EncodedVideoCodec,
) -> Option<GStreamerEncoderRateControl> {
    let encoder = pipeline.by_name(GSTREAMER_ENCODER_NAME)?;
    let (property, unit) = match codec {
        EncodedVideoCodec::H264 | EncodedVideoCodec::H265 => {
            ("bitrate", GStreamerBitrateUnit::KilobitsPerSecond)
        }
        EncodedVideoCodec::VP8 | EncodedVideoCodec::VP9 => {
            ("target-bitrate", GStreamerBitrateUnit::BitsPerSecond)
        }
        EncodedVideoCodec::AV1 => ("target-bitrate", GStreamerBitrateUnit::KilobitsPerSecond),
        _ => return None,
    };
    Some(GStreamerEncoderRateControl::new(encoder, property, unit))
}

#[cfg(feature = "gstreamer")]
fn update_bitrate_overlay(overlay: &gst::Element, rate_control: EncodedRateControl) {
    let Some(pspec) = overlay.find_property("text") else {
        log::warn!(
            "GStreamer element '{}' has no text property for bitrate overlay",
            overlay.name()
        );
        return;
    };

    let flags = pspec.flags();
    if pspec.value_type() != String::static_type()
        || !flags.contains(glib::ParamFlags::WRITABLE)
        || flags.contains(glib::ParamFlags::CONSTRUCT_ONLY)
    {
        log::warn!(
            "GStreamer element '{}' cannot be used as a bitrate text overlay",
            overlay.name()
        );
        return;
    }

    overlay.set_property("text", bitrate_overlay_text(rate_control));
}

#[cfg(feature = "gstreamer")]
fn bitrate_overlay_text(rate_control: EncodedRateControl) -> String {
    format!(
        "WebRTC target: {} kbps @ {:.1} fps",
        rate_control.target_bitrate_bps / 1000,
        rate_control.framerate_fps
    )
}

#[cfg(feature = "gstreamer")]
fn gstreamer_test_encode_pipeline(fps: u32, codec: EncodedVideoCodec, bitrate: u64) -> String {
    let key_int_max = fps.max(1);
    let bitrate_kbps = u64::max(1, bitrate / 1000);
    // The trailing capsfilter is the appsink contract, so it comes from the
    // crate's caps table; encoder-specific settings before the parser stay
    // inline because they configure the encoder, not the appsink.
    let caps = encoded_caps_string(codec);
    match codec {
        EncodedVideoCodec::H264 => format!(
            "x264enc name={GSTREAMER_ENCODER_NAME} tune=zerolatency speed-preset=ultrafast \
             key-int-max={key_int_max} \
             bitrate={bitrate_kbps} byte-stream=true aud=true ! h264parse config-interval=-1 ! \
             {caps}"
        ),
        EncodedVideoCodec::H265 => format!(
            "x265enc name={GSTREAMER_ENCODER_NAME} tune=zerolatency speed-preset=ultrafast \
             key-int-max={key_int_max} \
             bitrate={bitrate_kbps} option-string=repeat-headers=1:aud=1:open-gop=0 ! \
             h265parse config-interval=-1 ! {caps}"
        ),
        EncodedVideoCodec::VP8 => format!(
            "vp8enc name={GSTREAMER_ENCODER_NAME} deadline=1 cpu-used=8 \
             keyframe-max-dist={key_int_max} lag-in-frames=0 target-bitrate={bitrate} ! {caps}"
        ),
        EncodedVideoCodec::VP9 => format!(
            "vp9enc name={GSTREAMER_ENCODER_NAME} deadline=1 cpu-used=8 \
             keyframe-max-dist={key_int_max} lag-in-frames=0 target-bitrate={bitrate} ! {caps}"
        ),
        EncodedVideoCodec::AV1 => format!(
            "av1enc name={GSTREAMER_ENCODER_NAME} cpu-used=8 usage-profile=realtime \
             keyframe-max-dist={key_int_max} lag-in-frames=0 target-bitrate={bitrate_kbps} ! \
             av1parse ! {caps}"
        ),
        _ => unreachable!("unknown generated GStreamer codec"),
    }
}

#[cfg(feature = "gstreamer")]
fn gstreamer_shmsink_pipeline_description(socket_path: &str, codec: EncodedVideoCodec) -> String {
    let socket_path = gstreamer_launch_string_value(socket_path);
    let caps = encoded_caps_string(codec);

    format!(
        "shmsrc socket-path={socket_path} is-live=true do-timestamp=true ! capsfilter caps={caps}"
    )
}

#[cfg(feature = "gstreamer")]
fn gstreamer_launch_string_value(value: &str) -> String {
    if value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | ':'))
    {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
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
    let metadata_config = FrameMetadataConfig {
        attach_timestamp: args.attach_timestamp,
        attach_frame_id: args.attach_frame_id,
    };
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

    let capture_track = VideoCaptureTrack::new_encoded(
        "preencoded",
        VideoResolution { width: args.width, height: args.height },
    );
    let mut publish_options = VideoCaptureTrack::encoded_publish_options(codec);
    let video_encoding =
        publish_video_encoding(args.max_bitrate, args.width, args.height, args.fps, codec);
    publish_options.video_encoding = Some(video_encoding.clone());
    publish_options.source = TrackSource::Camera;
    publish_options.frame_metadata_features = metadata_config.publish_features();

    room.local_participant()
        .publish_track(LocalTrack::Video(capture_track.track()), publish_options)
        .await
        .context("failed to publish pre-encoded video track")?;
    log::info!(
        "Published pre-encoded {:?} track at {}x{} (max_bitrate={}bps max_framerate={:.1}); forwarding {} access units",
        codec,
        args.width,
        args.height,
        video_encoding.max_bitrate,
        video_encoding.max_framerate,
        source_label
    );
    if metadata_config.is_enabled() {
        log::info!(
            "Frame metadata enabled: timestamp={} frame_id={}",
            metadata_config.attach_timestamp,
            metadata_config.attach_frame_id
        );
    }

    let keyframe_requests_forwarded = Arc::new(AtomicU64::new(0));
    let ingress = EncodedIngress::new(
        capture_track,
        KeyframeRequestLogger::new(source, source_label, keyframe_requests_forwarded.clone()),
    );
    let stop = ingress.stop_handle();
    let signal_task = tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        stop.stop();
        shutdown_source();
    });

    let capture_task = tokio::task::spawn_blocking(move || {
        let diagnostics = AccessUnitDiagnostics::new(
            diagnostics_enabled,
            source_label,
            expected_frame_interval_us,
            keyframe_requests_forwarded,
        );
        forward_access_units(ingress, diagnostics, metadata_config)
    });
    let captured = capture_task.await.context("capture task failed to join")??;
    signal_task.abort();
    room.close().await.context("failed to close LiveKit room")?;

    log::info!("Stopped after publishing {captured} encoded access units");
    Ok(())
}

/// Drives [`EncodedIngress::capture_next`] until EOF or shutdown, feeding the
/// example's per-access-unit diagnostics from each capture.
fn forward_access_units<S>(
    mut ingress: EncodedIngress<S>,
    mut diagnostics: AccessUnitDiagnostics,
    metadata_config: FrameMetadataConfig,
) -> Result<u64>
where
    S: EncodedAccessUnitSource,
{
    let stop = ingress.stop_handle();
    let mut pacer = AccessUnitPacer::default();
    let mut captured = 0;
    let mut dropped = 0;
    let mut frame_counter = 0_u32;
    while !stop.is_stopped() {
        let read_started = Instant::now();
        let capture = match ingress
            .capture_next_with_metadata(|_| metadata_config.next_metadata(&mut frame_counter))
        {
            Ok(Some(capture)) => capture,
            Ok(None) => break,
            Err(EncodedIngressError::Capture(CaptureError::CaptureFailed)) => {
                dropped += 1;
                if dropped == 1 || dropped % 300 == 0 {
                    log::info!("Dropped {dropped} encoded access units before capture");
                }
                continue;
            }
            Err(EncodedIngressError::Source(err)) if stop.is_stopped() => {
                log::debug!("encoded source stopped after shutdown: {err}");
                break;
            }
            Err(err) => return Err(err.into()),
        };
        diagnostics.observe_source_wait(read_started.elapsed());
        diagnostics.observe_capture(&capture);
        captured += 1;
        if captured % 300 == 0 {
            log::info!("Published {captured} encoded access units");
        }
        pacer.sleep_after_capture(&capture, &stop);
    }
    diagnostics.finish();

    Ok(captured)
}

#[derive(Debug, Clone, Copy, Default)]
struct FrameMetadataConfig {
    attach_timestamp: bool,
    attach_frame_id: bool,
}

impl FrameMetadataConfig {
    fn is_enabled(&self) -> bool {
        self.attach_timestamp || self.attach_frame_id
    }

    fn publish_features(&self) -> FrameMetadataFeatures {
        let mut features = FrameMetadataFeatures::default();
        features.user_timestamp = self.attach_timestamp;
        features.frame_id = self.attach_frame_id;
        features
    }

    fn next_metadata(&self, frame_counter: &mut u32) -> Option<FrameMetadata> {
        if !self.is_enabled() {
            return None;
        }

        let user_timestamp = self.attach_timestamp.then(|| current_time_us().max(0) as u64);
        let frame_id = if self.attach_frame_id {
            let frame_id = *frame_counter;
            *frame_counter = (*frame_counter).wrapping_add(1);
            Some(frame_id)
        } else {
            None
        };

        Some(FrameMetadata { user_timestamp, frame_id, user_data: None })
    }
}

#[derive(Debug, Default)]
struct AccessUnitPacer {
    start_timestamp_us: Option<i64>,
    start_wall_time: Option<Instant>,
}

impl AccessUnitPacer {
    fn sleep_after_capture(&mut self, capture: &EncodedIngressCapture, stop: &EncodedIngressStop) {
        let Some(mut remaining) = self.delay_after_capture(capture, Instant::now()) else {
            return;
        };

        while !remaining.is_zero() && !stop.is_stopped() {
            let sleep_for = remaining.min(PUBLISH_PACE_SLEEP_SLICE);
            std::thread::sleep(sleep_for);
            remaining = remaining.saturating_sub(sleep_for);
        }
    }

    fn delay_after_capture(
        &mut self,
        capture: &EncodedIngressCapture,
        now: Instant,
    ) -> Option<Duration> {
        let (Some(start_timestamp_us), Some(start_wall_time)) =
            (self.start_timestamp_us, self.start_wall_time)
        else {
            self.start_timestamp_us = Some(capture.timestamp_us);
            self.start_wall_time = Some(now);
            return None;
        };

        let elapsed_us = capture.timestamp_us.saturating_sub(start_timestamp_us);
        if elapsed_us <= 0 {
            return None;
        }

        let elapsed = Duration::from_micros(elapsed_us as u64);
        let target = start_wall_time + elapsed;
        if target <= now {
            return None;
        }

        Some(target.saturating_duration_since(now).min(MAX_PUBLISH_PACE_SLEEP))
    }
}

/// Wraps an encoded source to count and log the downstream keyframe requests
/// (PLI/FIR polled by [`EncodedIngress::capture_next`]) forwarded to it.
struct KeyframeRequestLogger<S> {
    source: S,
    source_label: &'static str,
    forwarded: Arc<AtomicU64>,
    last_forwarded: Option<Instant>,
}

impl<S> KeyframeRequestLogger<S> {
    fn new(source: S, source_label: &'static str, forwarded: Arc<AtomicU64>) -> Self {
        Self { source, source_label, forwarded, last_forwarded: None }
    }

    fn should_forward(&mut self, now: Instant) -> bool {
        let Some(last_forwarded) = self.last_forwarded else {
            self.last_forwarded = Some(now);
            return true;
        };

        if now.saturating_duration_since(last_forwarded) < MIN_KEYFRAME_REQUEST_INTERVAL {
            return false;
        }

        self.last_forwarded = Some(now);
        true
    }
}

impl<S> EncodedAccessUnitSource for KeyframeRequestLogger<S>
where
    S: EncodedAccessUnitSource,
{
    type Error = S::Error;

    fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
        self.source.next_access_unit()
    }

    fn request_keyframe(&mut self) {
        if !self.should_forward(Instant::now()) {
            return;
        }
        let forwarded = self.forwarded.fetch_add(1, Ordering::Relaxed) + 1;
        log::info!(
            "{} forwarding downstream keyframe request {forwarded} to the encoded source",
            self.source_label
        );
        self.source.request_keyframe();
    }

    fn update_rate_control(&mut self, rate_control: EncodedRateControl) {
        self.source.update_rate_control(rate_control);
    }
}

#[derive(Debug)]
struct AccessUnitDiagnostics {
    enabled: bool,
    source_label: &'static str,
    expected_frame_interval_us: Option<i64>,
    keyframe_requests_forwarded: Arc<AtomicU64>,
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
}

impl AccessUnitDiagnostics {
    fn new(
        enabled: bool,
        source_label: &'static str,
        expected_frame_interval_us: Option<i64>,
        keyframe_requests_forwarded: Arc<AtomicU64>,
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
            keyframe_requests_forwarded,
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

    fn observe_capture(&mut self, capture: &EncodedIngressCapture) {
        if !self.enabled {
            return;
        }

        let now = Instant::now();
        let payload_len = capture.payload_len;
        let is_keyframe = capture.frame_type == EncodedFrameType::Key;
        let timestamp_gap_us =
            self.last_timestamp_us.map(|last| capture.timestamp_us.saturating_sub(last));

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
            log::info!(
                "{} keyframe {} ts={} size={}",
                self.source_label,
                self.total_frames,
                capture.timestamp_us,
                payload_len
            );
        }

        self.warn_if_keyframe_gap(now);
        self.last_wall_time = Some(now);
        self.last_timestamp_us = Some(capture.timestamp_us);
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
                "{} no keyframe for {:.1}s; {} downstream keyframe request(s) forwarded to the \
                 source so far",
                self.source_label,
                keyframe_gap.as_secs_f64(),
                self.keyframe_requests_forwarded.load(Ordering::Relaxed)
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
             bursts={} keyframe_requests={}",
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
            self.keyframe_requests_forwarded.load(Ordering::Relaxed)
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

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        bail!("--width and --height must be greater than zero");
    }
    Ok(())
}

fn validate_max_bitrate(max_bitrate: Option<u64>) -> Result<()> {
    if max_bitrate == Some(0) {
        bail!("--max-bitrate must be greater than zero");
    }
    Ok(())
}

fn frame_interval_us(fps: u32) -> Result<i64> {
    if fps == 0 {
        bail!("--fps must be greater than zero");
    }
    Ok(1_000_000_i64 / i64::from(fps))
}

fn publish_video_encoding(
    max_bitrate: Option<u64>,
    width: u32,
    height: u32,
    fps: u32,
    codec: EncodedVideoCodec,
) -> VideoEncoding {
    let mut encoding = options::compute_appropriate_encoding(false, width, height, codec.into());
    if let Some(max_bitrate) = max_bitrate {
        encoding.max_bitrate = max_bitrate;
    } else {
        encoding.max_bitrate =
            encoding.max_bitrate.saturating_mul(preencoded_bitrate_headroom(codec));
    }
    encoding.max_framerate = f64::from(fps);
    encoding
}

fn preencoded_bitrate_headroom(codec: EncodedVideoCodec) -> u64 {
    match codec {
        EncodedVideoCodec::H265 => H265_PREENCODED_BITRATE_HEADROOM,
        EncodedVideoCodec::AV1 => AV1_PREENCODED_BITRATE_HEADROOM,
        _ => 1,
    }
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
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::H264, None, false);

        assert!(description.contains("videotestsrc is-live=true do-timestamp=true"));
        assert!(description.contains("pattern=ball motion=wavy animation-mode=frames"));
        assert!(description.contains("timeoverlay"));
        assert!(!description.contains(GSTREAMER_BITRATE_OVERLAY_NAME));
        assert!(description.contains("video/x-raw,format=I420"));
        assert!(description.contains(&format!("x264enc name={GSTREAMER_ENCODER_NAME}")));
        assert!(description.contains("video/x-h264,stream-format=byte-stream,alignment=au"));
        assert!(description.contains(&format!("appsink name={ENCODED_APPSINK_NAME}")));
    }

    #[test]
    fn gstreamer_pipeline_description_routes_test_source_to_h265_appsink() {
        let description =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::H265, None, false);

        assert!(description.contains("videotestsrc is-live=true do-timestamp=true"));
        assert!(description.contains("timeoverlay"));
        assert!(description.contains("video/x-raw,format=I420"));
        assert!(description.contains(&format!("x265enc name={GSTREAMER_ENCODER_NAME}")));
        assert!(description.contains("bitrate=360"));
        assert!(description.contains("option-string=repeat-headers=1:aud=1:open-gop=0"));
        assert!(description.contains("h265parse config-interval=-1"));
        assert!(description.contains("video/x-h265,stream-format=byte-stream,alignment=au"));
        assert!(description.contains(&format!("appsink name={ENCODED_APPSINK_NAME}")));
    }

    #[test]
    fn gstreamer_pipeline_description_can_overlay_webrtc_bitrate() {
        let description =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::H264, None, true);

        assert!(description.contains(&format!("textoverlay name={GSTREAMER_BITRATE_OVERLAY_NAME}")));
        assert!(description.contains("text=\"WebRTC target: pending\""));
        assert!(description.contains("halignment=left valignment=top"));
    }

    #[test]
    fn preencoded_publish_encoding_adds_codec_headroom() {
        let h264 = publish_video_encoding(None, 640, 480, 30, EncodedVideoCodec::H264);
        let h265 = publish_video_encoding(None, 640, 480, 30, EncodedVideoCodec::H265);
        let av1 = publish_video_encoding(None, 640, 480, 30, EncodedVideoCodec::AV1);
        let explicit_h265 =
            publish_video_encoding(Some(450_000), 640, 480, 30, EncodedVideoCodec::H265);
        let explicit_av1 =
            publish_video_encoding(Some(315_000), 640, 480, 30, EncodedVideoCodec::AV1);

        assert_eq!(h264.max_bitrate, 450_000);
        assert_eq!(h265.max_bitrate, 900_000);
        assert_eq!(av1.max_bitrate, 945_000);
        assert_eq!(explicit_h265.max_bitrate, 450_000);
        assert_eq!(explicit_av1.max_bitrate, 315_000);
        assert_eq!(h265.max_framerate, 30.0);
    }

    #[test]
    fn access_unit_pacer_delays_when_source_runs_ahead_of_timestamps() {
        let mut pacer = AccessUnitPacer::default();
        let now = Instant::now();
        let first = encoded_capture(1_000_000);
        let second = encoded_capture(1_033_333);
        let distant = encoded_capture(2_000_000);

        assert_eq!(pacer.delay_after_capture(&first, now), None);
        assert_eq!(pacer.delay_after_capture(&second, now), Some(Duration::from_micros(33_333)));
        assert_eq!(pacer.delay_after_capture(&distant, now), Some(MAX_PUBLISH_PACE_SLEEP));
    }

    #[test]
    fn keyframe_request_logger_rate_limits_forwarding() {
        let mut logger = KeyframeRequestLogger::new((), "test", Arc::new(AtomicU64::new(0)));
        let now = Instant::now();

        assert!(logger.should_forward(now));
        assert!(!logger.should_forward(now + MIN_KEYFRAME_REQUEST_INTERVAL / 2));
        assert!(logger.should_forward(now + MIN_KEYFRAME_REQUEST_INTERVAL));
    }

    fn encoded_capture(timestamp_us: i64) -> EncodedIngressCapture {
        EncodedIngressCapture { timestamp_us, frame_type: EncodedFrameType::Delta, payload_len: 1 }
    }

    #[test]
    fn gstreamer_pipeline_description_routes_test_source_to_vp8_vp9_and_av1_appsink() {
        let vp8 =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::VP8, None, false);
        assert!(vp8.contains("video/x-raw,format=I420"));
        assert!(vp8.contains(&format!("vp8enc name={GSTREAMER_ENCODER_NAME}")));
        assert!(vp8.contains("video/x-vp8"));
        assert!(vp8.contains(&format!("appsink name={ENCODED_APPSINK_NAME}")));

        let vp9 =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::VP9, None, false);
        assert!(vp9.contains("video/x-raw,format=I420"));
        assert!(vp9.contains(&format!("vp9enc name={GSTREAMER_ENCODER_NAME}")));
        assert!(vp9.contains("video/x-vp9,profile=(string)0"));
        assert!(vp9.contains(&format!("appsink name={ENCODED_APPSINK_NAME}")));

        let av1 =
            gstreamer_test_pipeline_description(320, 180, 30, EncodedVideoCodec::AV1, None, false);
        assert!(av1.contains("video/x-raw,format=I420"));
        assert!(av1.contains(&format!("av1enc name={GSTREAMER_ENCODER_NAME}")));
        assert!(av1.contains("av1parse"));
        assert!(av1.contains("video/x-av1,stream-format=obu-stream,alignment=tu"));
        assert!(av1.contains(&format!("appsink name={ENCODED_APPSINK_NAME}")));
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
            gstreamer_pipeline_description(
                320,
                180,
                30,
                EncodedVideoCodec::H265,
                &pipeline,
                None,
                true,
            ),
            "videotestsrc is-live=true ! x264enc"
        );
    }

    #[test]
    fn gstreamer_shmsink_pipeline_description_uses_socket_path_and_codec_caps() {
        let h264 = gstreamer_shmsink_pipeline_description(
            "/tmp/livekit h264.shm",
            EncodedVideoCodec::H264,
        );
        assert!(h264.contains("shmsrc socket-path=\"/tmp/livekit h264.shm\""));
        assert!(h264.contains("is-live=true do-timestamp=true"));
        assert!(h264.contains("capsfilter caps="));
        assert!(h264.contains("video/x-h264,stream-format=byte-stream,alignment=au"));

        let vp8 =
            gstreamer_shmsink_pipeline_description("/tmp/livekit-vp8.shm", EncodedVideoCodec::VP8);
        assert!(vp8.contains("shmsrc socket-path=/tmp/livekit-vp8.shm"));
        assert!(vp8.contains("video/x-vp8"));

        let vp9 =
            gstreamer_shmsink_pipeline_description("/tmp/livekit-vp9.shm", EncodedVideoCodec::VP9);
        assert!(vp9.contains("video/x-vp9,profile=(string)0"));

        let av1 =
            gstreamer_shmsink_pipeline_description("/tmp/livekit-av1.shm", EncodedVideoCodec::AV1);
        assert!(av1.contains("video/x-av1,stream-format=obu-stream,alignment=tu"));
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
            None,
            false,
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
            None,
            false,
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
            None,
            false,
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
            None,
            false,
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
