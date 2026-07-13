use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{
    self, video as video_presets, FrameMetadataFeatures, TrackPublishOptions, VideoCodec,
    VideoEncoderBackend, VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{
    native::{NativeBuffer, VideoFrameBufferExt},
    FrameMetadata, I420Buffer, VideoFrame, VideoRotation,
};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use livekit_api::services::room::{CreateRoomOptions, RoomClient};
use livekit_api::services::{ServiceError, TwirpError, TwirpErrorCode};
use livekit_capture::device::{
    CaptureBackend, CaptureDeviceSelector, CaptureFormat as LkCaptureFormat, CaptureFormatRequest,
    CaptureFrameFormat, CapturePath as LkCapturePath, CaptureResolution,
};
use livekit_capture::source::{CaptureFrame, CaptureSourceOptions, VideoCaptureSource};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use livekit_capture::sources::argus::{self, ArgusCaptureOptions, ArgusCaptureSession};
#[cfg(target_os = "macos")]
use livekit_capture::sources::avfoundation::AvFoundationStopHandle;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod codec_display;
mod frame_log;
mod test_pattern;
mod timestamp_burn;
mod user_data;
mod video_display;
mod viewport_aspect;

use test_pattern::{TestPattern, TestPatternKind};
use timestamp_burn::TimestampOverlay;
use video_display::{align_up, PublisherTimingSample, SharedYuv};

use frame_log::{create_csv, CsvFloat, CsvLatency, CsvOption, FrameLogRange};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum PublisherCodec {
    H264,
    H265,
    VP8,
    VP9,
    AV1,
}

impl From<PublisherCodec> for VideoCodec {
    fn from(codec: PublisherCodec) -> Self {
        match codec {
            PublisherCodec::H264 => VideoCodec::H264,
            PublisherCodec::H265 => VideoCodec::H265,
            PublisherCodec::VP8 => VideoCodec::VP8,
            PublisherCodec::VP9 => VideoCodec::VP9,
            PublisherCodec::AV1 => VideoCodec::AV1,
        }
    }
}

/// Selects the camera backend used by the publisher.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum SourceKind {
    /// Platform camera via livekit-capture (AVFoundation on macOS, V4L2 on Linux).
    Uvc,
    /// NVIDIA Jetson MIPI CSI camera via libargus (Jetson-only).
    Argus,
}

/// Selects the UVC camera capture frame format.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum CaptureFormat {
    /// Prefer YUYV, falling back to other formats supported by the camera.
    Auto,
    /// Request uncompressed YUYV capture.
    Yuv,
    /// Request compressed MJPEG capture.
    Mjpeg,
    /// Request uncompressed GREY capture.
    Grey,
}

impl CaptureFormat {
    /// Preferred source frame format used for V4L2 format negotiation; the
    /// capture facade falls back to the camera's other supported formats when
    /// the preferred one is unavailable.
    #[cfg(target_os = "linux")]
    fn preferred_frame_format(self) -> CaptureFrameFormat {
        match self {
            Self::Auto | Self::Yuv => CaptureFrameFormat::Yuyv,
            Self::Mjpeg => CaptureFrameFormat::Mjpeg,
            Self::Grey => CaptureFrameFormat::Grey,
        }
    }
}

impl std::fmt::Display for CaptureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Yuv => write!(f, "yuv"),
            Self::Mjpeg => write!(f, "mjpeg"),
            Self::Grey => write!(f, "grey"),
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum PublisherEncoder {
    Auto,
    Software,
    Hardware,
    Nvenc,
    Vaapi,
    #[value(name = "videotoolbox")]
    VideoToolbox,
}

impl PublisherEncoder {
    fn as_str(&self) -> &'static str {
        match self {
            PublisherEncoder::Auto => "auto",
            PublisherEncoder::Software => "software",
            PublisherEncoder::Hardware => "hardware",
            PublisherEncoder::Nvenc => "nvenc",
            PublisherEncoder::Vaapi => "vaapi",
            PublisherEncoder::VideoToolbox => "videotoolbox",
        }
    }
}

impl From<PublisherEncoder> for VideoEncoderBackend {
    fn from(encoder: PublisherEncoder) -> Self {
        match encoder {
            PublisherEncoder::Auto => VideoEncoderBackend::Auto,
            PublisherEncoder::Software => VideoEncoderBackend::Software,
            PublisherEncoder::Hardware => VideoEncoderBackend::Hardware,
            PublisherEncoder::Nvenc => VideoEncoderBackend::Nvenc,
            PublisherEncoder::Vaapi => VideoEncoderBackend::Vaapi,
            PublisherEncoder::VideoToolbox => VideoEncoderBackend::VideoToolbox,
        }
    }
}

fn video_encoder_backend_name(backend: VideoEncoderBackend) -> &'static str {
    match backend {
        VideoEncoderBackend::Auto => "auto",
        VideoEncoderBackend::Software => "software",
        VideoEncoderBackend::Hardware => "hardware",
        VideoEncoderBackend::Nvenc => "nvenc",
        VideoEncoderBackend::Vaapi => "vaapi",
        VideoEncoderBackend::VideoToolbox => "videotoolbox",
        VideoEncoderBackend::PreEncoded => "preencoded",
        _ => "unknown",
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// List available video encoder backends and exit
    #[arg(long)]
    list_encoders: bool,

    /// Camera index to use (numeric)
    #[arg(long, default_value_t = 0)]
    camera_index: usize,

    /// Camera backend: `uvc` (default platform camera) or `argus` (Jetson MIPI CSI).
    #[arg(long, value_enum, default_value_t = SourceKind::Uvc)]
    source: SourceKind,

    /// UVC camera capture format: `auto` prefers YUYV and falls back to other supported formats.
    #[arg(long, value_enum, default_value_t = CaptureFormat::Auto)]
    format: CaptureFormat,

    /// Use zero-copy platform camera buffers when available.
    #[arg(long, default_value_t = false)]
    zero_copy: bool,

    /// Generate a numeric test pattern instead of using a camera: 0 = static bars, 1 = animated
    #[arg(
        long,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "0",
        value_parser = parse_test_pattern_kind,
        conflicts_with_all = ["list_cameras", "list_encoders"]
    )]
    test_pattern: Option<TestPatternKind>,

    /// Desired width
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Desired height
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Desired framerate
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Max video bitrate for the main layer in bps (optional)
    #[arg(long)]
    max_bitrate: Option<u64>,

    /// Enable simulcast publishing (low/medium/high layers as appropriate)
    #[arg(long, default_value_t = false)]
    simulcast: bool,

    /// LiveKit participant identity
    #[arg(long, default_value = "rust-camera-pub")]
    identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")]
    room_name: String,

    /// Minimum subscriber playout delay in milliseconds; recreates the room when set
    #[arg(long)]
    min_playout_delay: Option<u32>,

    /// Maximum subscriber playout delay in milliseconds; recreates the room when set
    #[arg(long)]
    max_playout_delay: Option<u32>,

    /// LiveKit server URL
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret
    #[arg(long)]
    api_secret: Option<String>,

    /// Video codec to use for publishing
    #[arg(long, value_enum, default_value_t = PublisherCodec::H264)]
    codec: PublisherCodec,

    /// Preferred video encoder backend to use for publishing
    #[arg(long, value_enum, default_value_t = PublisherEncoder::Auto)]
    encoder: PublisherEncoder,

    /// Attach the current system time (microseconds since UNIX epoch) as the user timestamp on each frame
    #[arg(long, default_value_t = false)]
    attach_timestamp: bool,

    /// Enable dynacast (pause unused simulcast layers based on subscriber demand)
    #[arg(long, default_value_t = false)]
    dynacast: bool,

    /// Burn the attached timestamp into each video frame; does nothing unless --attach-timestamp is also enabled
    #[arg(long, default_value_t = false)]
    burn_timestamp: bool,

    /// Attach a monotonically increasing frame ID to each published frame via the packet trailer
    #[arg(long, default_value_t = false)]
    attach_frame_id: bool,

    /// Attach keyboard-controlled 6-channel data (6x int16 fixed-point, 12 bytes)
    /// as the per-frame user_data trailer field. Control the channels from the
    /// preview window: Q/A=CH1, W/S=CH2, E/D=CH3, R/F=CH4, T/G=CH5, Y/H=CH6.
    /// Requires --display-video (the window provides keyboard focus).
    #[arg(long, default_value_t = false, requires = "display_video")]
    attach_user_data: bool,

    /// Open a window that displays the video frames being published
    #[arg(long, default_value_t = false)]
    display_video: bool,

    /// Burn publisher timing metrics into the local preview window
    #[arg(long, default_value_t = false, requires = "display_video")]
    display_timing: bool,

    /// Write one row of publisher timing metrics per packetized frame to this CSV file
    #[arg(long, value_name = "PATH")]
    log_csv: Option<PathBuf>,

    /// Start CSV logging at this frame ID (inclusive)
    #[arg(long, requires = "log_csv")]
    log_start_frame_id: Option<u32>,

    /// Stop CSV logging after this frame ID (inclusive)
    #[arg(long, requires = "log_csv")]
    log_end_frame_id: Option<u32>,

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}

fn is_twirp_not_found(err: &ServiceError) -> bool {
    matches!(
        err,
        ServiceError::Twirp(TwirpError::Twirp(code))
            if code.code == TwirpErrorCode::NOT_FOUND
    )
}

fn requested_playout_delay(
    min_playout_delay: Option<u32>,
    max_playout_delay: Option<u32>,
) -> Option<(u32, u32)> {
    match (min_playout_delay, max_playout_delay) {
        (None, None) => None,
        (min_playout_delay, max_playout_delay) => {
            Some((min_playout_delay.unwrap_or_default(), max_playout_delay.unwrap_or_default()))
        }
    }
}

fn parse_test_pattern_kind(value: &str) -> Result<TestPatternKind, String> {
    let numeric =
        value.parse::<u8>().map_err(|_| format!("test pattern must be 0 or 1, got `{value}`"))?;
    TestPatternKind::try_from(numeric)
        .map_err(|_| format!("test pattern must be 0 or 1, got `{value}`"))
}

fn normalize_twirp_host(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("wss://") {
        return format!("https://{}", rest.trim_end_matches("/rtc"));
    }
    if let Some(rest) = url.strip_prefix("ws://") {
        return format!("http://{}", rest.trim_end_matches("/rtc"));
    }
    url.trim_end_matches("/rtc").to_string()
}

fn capture_path_name(path: LkCapturePath) -> &'static str {
    match path {
        LkCapturePath::Native => "native platform buffer",
        LkCapturePath::Raw => "CPU I420",
        LkCapturePath::DmaBuf => "DMA-BUF",
        LkCapturePath::Encoded => "pre-encoded",
        _ => "unknown",
    }
}

#[derive(Default)]
struct RollingMs {
    total_ms: f64,
    samples: u64,
}

impl RollingMs {
    fn record(&mut self, value_ms: f64) {
        self.total_ms += value_ms;
        self.samples += 1;
    }

    fn average(&self) -> Option<f64> {
        (self.samples > 0).then_some(self.total_ms / self.samples as f64)
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
struct PublisherTimingSummary {
    paced_wait_ms: RollingMs,
    camera_frame_read_ms: RollingMs,
    capture_timestamp_age_ms: RollingMs,
    capture_timestamp_to_webrtc_ms: RollingMs,
    frame_draw_ms: RollingMs,
    submit_to_webrtc_ms: RollingMs,
    capture_to_webrtc_total_ms: RollingMs,
}

fn find_video_outbound_encoder(stats: &[livekit::webrtc::stats::RtcStats]) -> Option<&str> {
    let mut fallback = None;
    for stat in stats {
        let livekit::webrtc::stats::RtcStats::OutboundRtp(outbound) = stat else {
            continue;
        };
        if outbound.stream.kind != "video" || outbound.outbound.encoder_implementation.is_empty() {
            continue;
        }

        let implementation = outbound.outbound.encoder_implementation.as_str();
        if outbound.outbound.active {
            return Some(implementation);
        }
        fallback.get_or_insert(implementation);
    }

    fallback
}

fn find_video_outbound_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
) -> Option<livekit::webrtc::stats::OutboundRtpStats> {
    let mut fallback = None;
    for stat in stats {
        let livekit::webrtc::stats::RtcStats::OutboundRtp(outbound) = stat else {
            continue;
        };
        if outbound.stream.kind != "video" {
            continue;
        }
        if outbound.outbound.active {
            return Some(outbound.clone());
        }
        fallback.get_or_insert_with(|| outbound.clone());
    }
    fallback
}

fn log_publisher_outbound_health(stats: &[livekit::webrtc::stats::RtcStats]) {
    let Some(outbound) = find_video_outbound_stats(stats) else {
        return;
    };

    info!(
        "Publish health: encoded={}, sent={}, keyframes={}, packets_sent={}, bytes_sent={}, pli={}, fir={}, encoder={}",
        outbound.outbound.frames_encoded,
        outbound.outbound.frames_sent,
        outbound.outbound.key_frames_encoded,
        outbound.sent.packets_sent,
        outbound.sent.bytes_sent,
        outbound.outbound.pli_count,
        outbound.outbound.fir_count,
        outbound.outbound.encoder_implementation,
    );

    if outbound.outbound.frames_encoded > 0 && outbound.sent.packets_sent == 0 {
        log::warn!(
            "Encoder produced frames but no RTP packets were sent; the AV1 bitstream may be malformed"
        );
    }
    if outbound.outbound.key_frames_encoded == 0 && outbound.outbound.pli_count > 0 {
        log::warn!(
            "Remote side requested keyframes (PLI={}) but the publisher has not encoded any keyframes",
            outbound.outbound.pli_count
        );
    }
}

fn maybe_request_zero_copy_fallback(
    outbound: &livekit::webrtc::stats::OutboundRtpStats,
    first_starved_at: &mut Option<Instant>,
    zero_copy_fallback: &AtomicBool,
) {
    if zero_copy_fallback.load(Ordering::Acquire) {
        return;
    }
    if outbound.outbound.frames_encoded > 0 || outbound.outbound.key_frames_encoded > 0 {
        *first_starved_at = None;
        return;
    }
    if outbound.outbound.pli_count == 0 && outbound.outbound.fir_count == 0 {
        return;
    }

    let starved_at = first_starved_at.get_or_insert_with(Instant::now);
    if starved_at.elapsed() < Duration::from_secs(3)
        && outbound.outbound.pli_count < 3
        && outbound.outbound.fir_count == 0
    {
        return;
    }

    zero_copy_fallback.store(true, Ordering::Release);
    log::warn!(
        "Zero-copy AVFoundation CVPixelBuffer publish produced no encoded frames; falling back to CPU I420 capture"
    );
}

async fn update_publisher_video_stats(
    track: LocalVideoTrack,
    ctrl_c_received: Arc<AtomicBool>,
    zero_copy_fallback: Option<Arc<AtomicBool>>,
) {
    let mut last_log =
        Instant::now().checked_sub(Duration::from_secs(2)).unwrap_or_else(Instant::now);
    let mut last_encoder_implementation = String::new();
    let mut zero_copy_starved_at = None;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        if let Ok(stats) = track.get_stats().await {
            if let Some(implementation) = find_video_outbound_encoder(&stats) {
                if implementation != last_encoder_implementation {
                    info!("Publisher encode path: WebRTC encoder implementation={implementation}");
                    last_encoder_implementation = implementation.to_string();
                }
            }
            if let (Some(outbound), Some(fallback)) =
                (find_video_outbound_stats(&stats), zero_copy_fallback.as_ref())
            {
                maybe_request_zero_copy_fallback(&outbound, &mut zero_copy_starved_at, fallback);
            }
            if last_log.elapsed() >= Duration::from_secs(2) {
                log_publisher_outbound_health(&stats);
                last_log = Instant::now();
            }
        }

        interval.tick().await;
    }
}

async fn update_publisher_encoder_overlay(
    track: LocalVideoTrack,
    shared: Arc<Mutex<SharedYuv>>,
    ctrl_c_received: Arc<AtomicBool>,
) {
    let mut logged_initial = false;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        match track.get_stats().await {
            Ok(stats) => {
                if let Some(implementation) = find_video_outbound_encoder(&stats) {
                    let mut shared = shared.lock();
                    shared.codec_implementation = implementation.to_string();
                }
                logged_initial = true;
            }
            Err(e) if !logged_initial => {
                debug!("Failed to get publisher stats for video track: {:?}", e);
                logged_initial = true;
            }
            Err(_) => {}
        }

        interval.tick().await;
    }
}

impl PublisherTimingSummary {
    fn reset(&mut self) {
        self.paced_wait_ms.reset();
        self.camera_frame_read_ms.reset();
        self.capture_timestamp_age_ms.reset();
        self.capture_timestamp_to_webrtc_ms.reset();
        self.frame_draw_ms.reset();
        self.submit_to_webrtc_ms.reset();
        self.capture_to_webrtc_total_ms.reset();
    }
}

fn format_timing_line(timings: &PublisherTimingSummary) -> String {
    let line_one = vec![
        format!("paced_wait {:.2}", timings.paced_wait_ms.average().unwrap_or_default()),
        format!(
            "camera_frame_read {:.2}",
            timings.camera_frame_read_ms.average().unwrap_or_default()
        ),
        format!(
            "capture_ts_age {:.2}",
            timings.capture_timestamp_age_ms.average().unwrap_or_default()
        ),
        format!(
            "capture_ts_to_webrtc {:.2}",
            timings.capture_timestamp_to_webrtc_ms.average().unwrap_or_default()
        ),
    ];
    let mut line_two = Vec::new();

    if let Some(frame_draw_ms) = timings.frame_draw_ms.average() {
        line_two.push(format!("frame_draw {:.2}", frame_draw_ms));
    }
    line_two.push(format!(
        "submit_to_webrtc {:.2}",
        timings.submit_to_webrtc_ms.average().unwrap_or_default()
    ));
    line_two.push(format!(
        "capture_to_webrtc_total {:.2}",
        timings.capture_to_webrtc_total_ms.average().unwrap_or_default()
    ));

    format!("Timing ms: {}\nTiming ms: {}", line_one.join(" | "), line_two.join(" | "))
}

const MAX_PUBLISH_TIMING_SAMPLES: usize = 300;

const PUBLISHER_CSV_HEADER: &str = "sample,elapsed_ms,frame_id,capture_timestamp_us,frame_buffer_timestamp_us,encoder_upload_timestamp_us,encoder_output_timestamp_us,webrtc_packetize_timestamp_us,capture_to_buffer_ms,buffer_to_encoder_ms,encode_ms,encoder_to_packetize_ms,capture_to_packetize_ms,frame_id_gap,packetize_interval_ms";

struct PublisherCsvLogger {
    writer: BufWriter<std::fs::File>,
    range: FrameLogRange,
    first_packetize_timestamp_us: Option<u64>,
    previous_packetize_timestamp_us: Option<u64>,
    previous_frame_id: Option<u32>,
    sample_count: u64,
    last_flush: Instant,
}

impl PublisherCsvLogger {
    fn new(path: &Path, range: FrameLogRange) -> std::io::Result<Self> {
        Ok(Self {
            writer: create_csv(path, PUBLISHER_CSV_HEADER)?,
            range,
            first_packetize_timestamp_us: None,
            previous_packetize_timestamp_us: None,
            previous_frame_id: range.previous_to_start(),
            sample_count: 0,
            last_flush: Instant::now(),
        })
    }

    fn record(&mut self, sample: PublisherTimingSample) -> std::io::Result<()> {
        let Some(frame_id) = sample.frame_id else {
            return Ok(());
        };
        if !self.range.contains(frame_id) {
            return Ok(());
        }
        let Some(frame_buffer_timestamp_us) = sample.got_frame_buffer_timestamp_us else {
            return Ok(());
        };
        let Some(encoder_upload_timestamp_us) = sample.encoder_upload_timestamp_us else {
            return Ok(());
        };
        let Some(encoder_output_timestamp_us) = sample.encoder_output_timestamp_us else {
            return Ok(());
        };
        let Some(packetize_timestamp_us) = sample.webrtc_packetize_timestamp_us else {
            return Ok(());
        };

        let first_packetize_timestamp_us =
            *self.first_packetize_timestamp_us.get_or_insert(packetize_timestamp_us);
        let frame_id_gap = self
            .previous_frame_id
            .and_then(|previous| frame_id.checked_sub(previous))
            .and_then(|delta| delta.checked_sub(1));
        let packetize_interval_ms = self
            .previous_packetize_timestamp_us
            .and_then(|previous| packetize_timestamp_us.checked_sub(previous))
            .map(|interval_us| interval_us as f64 / 1_000.0);
        self.sample_count += 1;

        writeln!(
            self.writer,
            "{},{:.3},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.sample_count,
            packetize_timestamp_us.saturating_sub(first_packetize_timestamp_us) as f64 / 1_000.0,
            frame_id,
            sample.sensor_exposure_timestamp_us,
            frame_buffer_timestamp_us,
            encoder_upload_timestamp_us,
            encoder_output_timestamp_us,
            packetize_timestamp_us,
            CsvLatency::between(
                Some(sample.sensor_exposure_timestamp_us),
                Some(frame_buffer_timestamp_us),
            ),
            CsvLatency::between(Some(frame_buffer_timestamp_us), Some(encoder_upload_timestamp_us),),
            CsvLatency::between(
                Some(encoder_upload_timestamp_us),
                Some(encoder_output_timestamp_us),
            ),
            CsvLatency::between(Some(encoder_output_timestamp_us), Some(packetize_timestamp_us),),
            CsvLatency::between(
                Some(sample.sensor_exposure_timestamp_us),
                Some(packetize_timestamp_us),
            ),
            CsvOption(frame_id_gap),
            CsvFloat(packetize_interval_ms),
        )?;

        self.previous_frame_id = Some(frame_id);
        self.previous_packetize_timestamp_us = Some(packetize_timestamp_us);
        if self.range.reaches_end(frame_id) || self.last_flush.elapsed() >= Duration::from_secs(1) {
            self.writer.flush()?;
            self.last_flush = Instant::now();
        }
        Ok(())
    }
}

#[derive(Default)]
struct PublisherTimingState {
    samples: HashMap<u64, PublisherTimingSample>,
    order: VecDeque<u64>,
    latest_complete_sample: Option<PublisherTimingSample>,
    frame_log: Option<PublisherCsvLogger>,
}

impl PublisherTimingState {
    fn record_frame_buffer(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        got_frame_buffer_timestamp_us: u64,
        frame_id: Option<u32>,
    ) -> PublisherTimingSample {
        let sample = self.get_or_insert_sample(sensor_exposure_timestamp_us, frame_id);
        sample.got_frame_buffer_timestamp_us = Some(got_frame_buffer_timestamp_us);
        *sample
    }

    fn record_sdk_event(&mut self, event: PublishTimingEvent) -> Option<PublisherTimingSample> {
        if event.capture_timestamp_us == 0 {
            return None;
        }

        let updated_sample = {
            let sample = self.get_or_insert_sample(event.capture_timestamp_us, event.frame_id);
            match event.stage {
                PublishTimingStage::EncoderUpload => {
                    sample.encoder_upload_timestamp_us = Some(event.timestamp_us);
                }
                PublishTimingStage::EncoderOutput => {
                    sample.encoder_output_timestamp_us = Some(event.timestamp_us);
                }
                PublishTimingStage::WebrtcPacketize => {
                    sample.webrtc_packetize_timestamp_us = Some(event.timestamp_us);
                }
            }
            *sample
        };

        if updated_sample.is_complete() {
            self.latest_complete_sample = Some(updated_sample);
            if let Some(frame_log) = self.frame_log.as_mut() {
                if let Err(error) = frame_log.record(updated_sample) {
                    warn!("Publisher CSV logging disabled after write failure: {error}");
                    self.frame_log = None;
                }
            }
            Some(updated_sample)
        } else {
            None
        }
    }

    fn display_sample(&self) -> Option<PublisherTimingSample> {
        self.latest_complete_sample
    }

    fn get_or_insert_sample(
        &mut self,
        sensor_exposure_timestamp_us: u64,
        frame_id: Option<u32>,
    ) -> &mut PublisherTimingSample {
        if !self.samples.contains_key(&sensor_exposure_timestamp_us) {
            self.samples.insert(
                sensor_exposure_timestamp_us,
                PublisherTimingSample::new(sensor_exposure_timestamp_us, frame_id),
            );
            self.order.push_back(sensor_exposure_timestamp_us);
            self.prune();
        }

        let sample = self
            .samples
            .get_mut(&sensor_exposure_timestamp_us)
            .expect("timing sample should exist after insertion");
        if frame_id.is_some() {
            sample.frame_id = frame_id;
        }
        sample
    }

    fn prune(&mut self) {
        while self.order.len() > MAX_PUBLISH_TIMING_SAMPLES {
            if let Some(oldest) = self.order.pop_front() {
                self.samples.remove(&oldest);
                if self
                    .latest_complete_sample
                    .is_some_and(|sample| sample.sensor_exposure_timestamp_us == oldest)
                {
                    self.latest_complete_sample = None;
                }
            }
        }
    }
}

fn update_shared_timing_sample(
    shared: Option<&Arc<Mutex<SharedYuv>>>,
    sample: PublisherTimingSample,
) {
    if let Some(shared) = shared {
        let mut shared = shared.lock();
        let should_update = shared.timing_sample.map_or(true, |current| {
            sample.sensor_exposure_timestamp_us >= current.sensor_exposure_timestamp_us
        });
        if should_update {
            shared.timing_sample = Some(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_frame_log_flags_parse_inclusive_bounds() {
        let args = Args::try_parse_from([
            "publisher",
            "--log-csv",
            "publisher.csv",
            "--log-start-frame-id",
            "301",
            "--log-end-frame-id",
            "1200",
        ])
        .expect("frame log flags should parse");
        assert_eq!(args.log_csv, Some(PathBuf::from("publisher.csv")));
        assert_eq!(args.log_start_frame_id, Some(301));
        assert_eq!(args.log_end_frame_id, Some(1200));
    }

    #[test]
    fn publisher_frame_log_bounds_require_csv_path() {
        assert!(Args::try_parse_from(["publisher", "--log-start-frame-id", "301"]).is_err());
    }

    #[test]
    fn publisher_frame_log_writes_complete_samples_in_range() {
        let path = std::env::temp_dir()
            .join(format!("local-video-publisher-frame-log-{}.csv", std::process::id()));
        let range = FrameLogRange::new(Some(301), Some(302)).expect("range should be valid");
        let mut logger = PublisherCsvLogger::new(&path, range).expect("log should be created");
        let sample = PublisherTimingSample {
            frame_id: Some(301),
            sensor_exposure_timestamp_us: 1_000,
            got_frame_buffer_timestamp_us: Some(1_100),
            encoder_upload_timestamp_us: Some(1_200),
            encoder_output_timestamp_us: Some(1_300),
            webrtc_packetize_timestamp_us: Some(1_400),
        };
        logger.record(sample).expect("sample should be written");
        logger.writer.flush().expect("log should flush");
        drop(logger);

        let contents = std::fs::read_to_string(&path).expect("log should be readable");
        let lines: Vec<_> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].split(',').count(), lines[1].split(',').count());
        assert!(lines[1].starts_with("1,0.000,301,"));
        std::fs::remove_file(path).expect("temporary log should be removable");
    }

    #[test]
    fn requested_playout_delay_is_absent_when_no_delay_flags_are_set() {
        assert_eq!(requested_playout_delay(None, None), None);
    }

    #[test]
    fn requested_playout_delay_defaults_unset_partial_delay() {
        assert_eq!(requested_playout_delay(Some(120), None), Some((120, 0)));
        assert_eq!(requested_playout_delay(None, Some(240)), Some((0, 240)));
        assert_eq!(requested_playout_delay(Some(120), Some(240)), Some((120, 240)));
    }

    fn timing_event(
        stage: PublishTimingStage,
        capture_timestamp_us: u64,
        timestamp_us: u64,
    ) -> PublishTimingEvent {
        PublishTimingEvent { stage, timestamp_us, capture_timestamp_us, frame_id: Some(7) }
    }

    #[test]
    fn publisher_timing_state_waits_for_complete_sample() {
        let mut state = PublisherTimingState::default();
        state.record_frame_buffer(1_000, 1_100, Some(7));

        assert!(state.display_sample().is_none());
        assert!(state
            .record_sdk_event(timing_event(PublishTimingStage::EncoderUpload, 1_000, 1_200))
            .is_none());
        assert!(state
            .record_sdk_event(timing_event(PublishTimingStage::EncoderOutput, 1_000, 1_300))
            .is_none());
        assert!(state.display_sample().is_none());
    }

    #[test]
    fn publisher_timing_state_displays_packetized_sample() {
        let mut state = PublisherTimingState::default();
        state.record_frame_buffer(1_000, 1_100, Some(7));
        state.record_sdk_event(timing_event(PublishTimingStage::EncoderUpload, 1_000, 1_200));
        state.record_sdk_event(timing_event(PublishTimingStage::EncoderOutput, 1_000, 1_300));

        let sample = state
            .record_sdk_event(timing_event(PublishTimingStage::WebrtcPacketize, 1_000, 1_400))
            .expect("packetized sample should be displayable");

        assert!(sample.is_complete());
        assert_eq!(state.display_sample().unwrap().webrtc_packetize_timestamp_us, Some(1_400));
    }

    #[test]
    fn publisher_timing_shared_update_accepts_current_frame() {
        let shared = Arc::new(Mutex::new(SharedYuv::default()));
        let mut current = PublisherTimingSample::new(1_000, Some(1));
        shared.lock().timing_sample = Some(current);

        current.encoder_upload_timestamp_us = Some(1_500);
        update_shared_timing_sample(Some(&shared), current);

        assert_eq!(shared.lock().timing_sample.unwrap().encoder_upload_timestamp_us, Some(1_500));
    }

    #[test]
    fn publisher_timing_shared_update_ignores_other_frames() {
        let shared = Arc::new(Mutex::new(SharedYuv::default()));
        let current = PublisherTimingSample::new(2_000, Some(2));
        let mut stale = PublisherTimingSample::new(1_000, Some(1));
        stale.encoder_upload_timestamp_us = Some(1_500);
        shared.lock().timing_sample = Some(current);

        update_shared_timing_sample(Some(&shared), stale);

        assert_eq!(
            shared.lock().timing_sample.unwrap().sensor_exposure_timestamp_us,
            current.sensor_exposure_timestamp_us
        );
    }

    #[test]
    fn test_pattern_is_absent_by_default() {
        let args = Args::try_parse_from(["publisher"]).expect("default args should parse");

        assert_eq!(args.test_pattern, None);
    }

    #[test]
    fn zero_copy_is_disabled_by_default() {
        let args = Args::try_parse_from(["publisher"]).expect("default args should parse");

        assert!(!args.zero_copy);
    }

    #[test]
    fn zero_copy_flag_enables_zero_copy() {
        let args = Args::try_parse_from(["publisher", "--zero-copy"]).expect("args should parse");

        assert!(args.zero_copy);
    }

    #[test]
    fn test_pattern_without_value_defaults_to_static_bars() {
        let args =
            Args::try_parse_from(["publisher", "--test-pattern"]).expect("args should parse");

        assert_eq!(args.test_pattern, Some(TestPatternKind::StaticColorBars));
    }

    #[test]
    fn test_pattern_without_value_allows_following_option() {
        let args = Args::try_parse_from(["publisher", "--test-pattern", "--room-name", "demo"])
            .expect("args should parse");

        assert_eq!(args.test_pattern, Some(TestPatternKind::StaticColorBars));
        assert_eq!(args.room_name, "demo");
    }

    #[test]
    fn test_pattern_accepts_numeric_mode() {
        let args =
            Args::try_parse_from(["publisher", "--test-pattern", "1"]).expect("args should parse");

        assert_eq!(args.test_pattern, Some(TestPatternKind::AnimatedGraphic));
    }

    #[test]
    fn capture_format_accepts_grey() {
        let args =
            Args::try_parse_from(["publisher", "--format", "grey"]).expect("args should parse");

        assert_eq!(args.format, CaptureFormat::Grey);
    }

    #[test]
    fn test_pattern_rejects_unknown_numeric_mode() {
        let err =
            Args::try_parse_from(["publisher", "--test-pattern", "2"]).expect_err("2 is invalid");

        assert!(err.to_string().contains("test pattern must be 0 or 1"));
    }
}

fn list_cameras() -> Result<()> {
    let cams = VideoCaptureSource::list_devices(CaptureBackend::Auto)?;
    println!("Available cameras:");
    for (i, cam) in cams.iter().enumerate() {
        println!("{}. {}", i, cam.name);
    }
    Ok(())
}

fn list_encoders() {
    println!("Available video encoder backends:");
    for backend in VideoEncoderBackend::list_available() {
        println!("- {}", video_encoder_backend_name(backend));
    }
}

enum VideoInput {
    TestPattern(TestPattern),
    /// Platform camera opened through the `livekit-capture` facade
    /// (AVFoundation on macOS, V4L2 on Linux).
    Camera(VideoCaptureSource),
    /// Jetson MIPI CSI camera driven directly so the `--zero-copy` CPU/DMA
    /// toggle stays available; see [`run_argus_capture_loop`].
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    Argus(ArgusCaptureSession),
}

/// Human-readable name of the backend behind a facade camera source.
fn camera_backend_name(source: &VideoCaptureSource) -> &'static str {
    match source {
        #[cfg(target_os = "macos")]
        VideoCaptureSource::AvFoundation { .. } => "AVFoundation",
        #[cfg(target_os = "linux")]
        VideoCaptureSource::V4l(_) => "V4L2",
        _ => "livekit-capture",
    }
}

fn publisher_capture_path_label(
    video_input: &VideoInput,
    burn_timestamp: bool,
    zero_copy: bool,
) -> String {
    match video_input {
        VideoInput::TestPattern(_) => "test-pattern CPU I420".to_string(),
        VideoInput::Camera(source) => match source {
            #[cfg(target_os = "macos")]
            VideoCaptureSource::AvFoundation { session, .. } => {
                let source_format = session.format().frame_format;
                let core_video_format = core_video_fourcc(session.core_video_pixel_format());
                if zero_copy {
                    match source.capture_path() {
                        LkCapturePath::Native if burn_timestamp => {
                            format!(
                                "AVFoundation zero-copy IOSurface CVPixelBuffer {core_video_format} from {source_format} (timestamp burn disabled)"
                            )
                        }
                        LkCapturePath::Native => {
                            format!(
                                "AVFoundation zero-copy IOSurface CVPixelBuffer {core_video_format} from {source_format}"
                            )
                        }
                        path => {
                            let suffix = if burn_timestamp {
                                "zero-copy unsupported, timestamp burn"
                            } else {
                                "zero-copy unsupported"
                            };
                            format!(
                                "AVFoundation {} fallback from {source_format}/{core_video_format} ({suffix})",
                                capture_path_name(path),
                            )
                        }
                    }
                } else if burn_timestamp {
                    format!(
                        "AVFoundation CPU I420 from {source_format}/{core_video_format} (timestamp burn)"
                    )
                } else {
                    format!("AVFoundation CPU I420 from {source_format}/{core_video_format}")
                }
            }
            #[cfg(target_os = "linux")]
            VideoCaptureSource::V4l(session) => {
                let format = session.format();
                let decode_suffix = if format.frame_format == CaptureFrameFormat::Mjpeg {
                    " with MJPEG decode"
                } else {
                    ""
                };
                if zero_copy {
                    let suffix = if burn_timestamp {
                        "zero-copy unsupported, timestamp burn"
                    } else {
                        "zero-copy unsupported"
                    };
                    format!(
                        "V4L2 {} fallback from {}{} ({suffix})",
                        capture_path_name(session.capture_path()),
                        format.frame_format,
                        decode_suffix
                    )
                } else {
                    format!(
                        "V4L2 {} from {}{}",
                        capture_path_name(session.capture_path()),
                        format.frame_format,
                        decode_suffix
                    )
                }
            }
            other => {
                format!(
                    "{} {} capture",
                    camera_backend_name(other),
                    capture_path_name(other.capture_path())
                )
            }
        },
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        VideoInput::Argus(_) => {
            if zero_copy && burn_timestamp {
                "libargus NV12 DMA-BUF zero-copy (timestamp burn disabled)".to_string()
            } else if zero_copy {
                "libargus NV12 DMA-BUF zero-copy".to_string()
            } else if burn_timestamp {
                "libargus CPU I420 from NV12 DMA-BUF (timestamp burn)".to_string()
            } else {
                "libargus CPU I420 from NV12 DMA-BUF".to_string()
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn core_video_fourcc(pixel_format: u32) -> String {
    let bytes = pixel_format.to_be_bytes();
    if bytes.iter().all(|byte| byte.is_ascii_graphic() || *byte == b' ') {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("0x{pixel_format:08x}")
    }
}

fn publisher_zero_copy_unsupported_reason(video_input: &VideoInput) -> Option<&'static str> {
    match video_input {
        VideoInput::TestPattern(_) => Some("test pattern frames are generated in CPU I420 memory"),
        VideoInput::Camera(source) => match source {
            #[cfg(target_os = "macos")]
            VideoCaptureSource::AvFoundation { .. } => {
                if source.capture_path() == LkCapturePath::Native {
                    None
                } else {
                    Some("the selected AVFoundation format is not IOSurface-backed NV12")
                }
            }
            #[cfg(target_os = "linux")]
            VideoCaptureSource::V4l(_) => {
                Some("V4L2 UVC capture does not expose a zero-copy capture/encode path here")
            }
            _ => Some(
                "the selected capture backend does not expose a zero-copy capture/encode path here",
            ),
        },
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        VideoInput::Argus(_) => None,
    }
}

fn publisher_zero_copy_supported(video_input: &VideoInput) -> bool {
    publisher_zero_copy_unsupported_reason(video_input).is_none()
}

fn publisher_uses_zero_copy_camera_capture(video_input: &VideoInput, zero_copy: bool) -> bool {
    if !zero_copy {
        return false;
    }

    match video_input {
        VideoInput::Camera(source) => source.capture_path() == LkCapturePath::Native,
        _ => false,
    }
}

enum CapturedFrameBuffer {
    I420(VideoFrame<I420Buffer>),
    #[cfg(target_os = "macos")]
    Native(VideoFrame<NativeBuffer>),
}

/// One frame obtained from the active video input, together with the timing
/// context the publish pipeline records.
struct SourcedFrame {
    buffer: CapturedFrameBuffer,
    /// Wall-clock capture timestamp in microseconds (camera-provided when available).
    capture_wall_time_us: u64,
    /// Wall-clock time the frame was read from the source, in microseconds.
    read_wall_time_us: u64,
    /// When the frame buffer became available to the publish pipeline.
    acquired_at: Instant,
    /// When work on this frame began; `capture_to_webrtc_total` is measured from here.
    pipeline_started_at: Instant,
    /// Whether `capture_wall_time_us` came from a camera-provided timestamp.
    has_camera_timestamp: bool,
}

fn sourced_frame_from_capture(frame: CaptureFrame) -> Result<SourcedFrame> {
    let acquired_at = Instant::now();
    match frame {
        CaptureFrame::Raw(raw) => Ok(SourcedFrame {
            has_camera_timestamp: raw.sensor_timestamp_us.is_some(),
            capture_wall_time_us: raw.capture_wall_time_us,
            read_wall_time_us: raw.read_wall_time_us,
            buffer: CapturedFrameBuffer::I420(raw.frame),
            acquired_at,
            pipeline_started_at: acquired_at,
        }),
        #[cfg(target_os = "macos")]
        CaptureFrame::Native(native) => Ok(SourcedFrame {
            has_camera_timestamp: native.sensor_timestamp_us.is_some(),
            capture_wall_time_us: native.capture_wall_time_us,
            read_wall_time_us: native.read_wall_time_us,
            buffer: CapturedFrameBuffer::Native(native.frame),
            acquired_at,
            pipeline_started_at: acquired_at,
        }),
        other => anyhow::bail!(
            "camera capture returned an unsupported {} frame",
            capture_path_name(other.capture_path())
        ),
    }
}

/// Cross-thread stop signal for a capture input blocked inside
/// [`VideoCaptureSource::next_frame`].
#[derive(Clone)]
enum CaptureStopHandle {
    /// AVFoundation wakes a blocked capture call via its stop handle.
    #[cfg(target_os = "macos")]
    AvFoundation(AvFoundationStopHandle),
    /// The input either never blocks (test pattern) or returns at the next
    /// frame boundary, where the loop observes the shutdown flag.
    FrameBoundary,
}

impl CaptureStopHandle {
    fn for_input(video_input: &VideoInput) -> Self {
        match video_input {
            #[cfg(target_os = "macos")]
            VideoInput::Camera(VideoCaptureSource::AvFoundation { session, .. }) => {
                Self::AvFoundation(session.stop_handle())
            }
            _ => Self::FrameBoundary,
        }
    }

    /// Interrupts a blocked `next_frame` when the backend supports it.
    fn stop(&self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::AvFoundation(handle) => handle.stop(),
            Self::FrameBoundary => {}
        }
    }
}

#[derive(Clone, Copy)]
struct CaptureConfig {
    fps: u32,
    /// Read by the Argus capture loop to pick DMA-BUF vs CPU I420 publish; the
    /// facade camera path bakes the zero-copy preference into the source when
    /// it is opened instead.
    #[cfg_attr(not(all(target_os = "linux", target_arch = "aarch64")), allow(dead_code))]
    zero_copy: bool,
    attach_timestamp: bool,
    burn_timestamp: bool,
    attach_frame_id: bool,
    display_timing: bool,
}

fn create_i420_buffer(width: u32, height: u32, align_for_display: bool) -> I420Buffer {
    if align_for_display {
        let uv_width = (width + 1) / 2;
        I420Buffer::with_strides(
            width,
            height,
            align_up(width, 256),
            align_up(uv_width, 256),
            align_up(uv_width, 256),
        )
    } else {
        I420Buffer::new(width, height)
    }
}

/// Opens the platform camera through the `livekit-capture` facade
/// (AVFoundation on macOS, V4L2 on Linux).
fn open_camera_source(args: &Args) -> Result<(u32, u32, VideoInput)> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        #[cfg(target_os = "macos")]
        let format_request = {
            if args.format != CaptureFormat::Auto {
                log::warn!(
                    "--format={} is ignored for AVFoundation decoded capture; AVFoundation supplies decoded CVPixelBuffers",
                    args.format
                );
            }
            CaptureFormatRequest::Closest(LkCaptureFormat::new(
                CaptureResolution::new(args.width, args.height),
                args.fps,
                CaptureFrameFormat::Nv12,
            ))
        };
        #[cfg(target_os = "linux")]
        let format_request = {
            let requested = LkCaptureFormat::new(
                CaptureResolution::new(args.width, args.height),
                args.fps,
                args.format.preferred_frame_format(),
            );
            if args.format == CaptureFormat::Auto {
                CaptureFormatRequest::Closest(requested)
            } else {
                CaptureFormatRequest::Exact(requested)
            }
        };

        // Without --zero-copy, ask for CPU-accessible frames so pixel work
        // (e.g. the --burn-timestamp overlay) is possible; with --zero-copy,
        // let AVFoundation deliver native platform buffers when supported.
        let source = VideoCaptureSource::open(CaptureSourceOptions {
            backend: CaptureBackend::Auto,
            device: CaptureDeviceSelector::Index(args.camera_index),
            format: format_request,
            prefer_raw_frames: !args.zero_copy,
            ..Default::default()
        })?;
        let format = source
            .format()
            .ok_or_else(|| anyhow::anyhow!("camera source did not report a negotiated format"))?;
        info!(
            "Camera opened with {}: {}x{} @ {} fps (source format: {}, requested: {}, camera {})",
            camera_backend_name(&source),
            format.resolution.width,
            format.resolution.height,
            format.frame_rate,
            format.frame_format,
            args.format,
            args.camera_index,
        );
        #[cfg(target_os = "linux")]
        if args.format != CaptureFormat::Auto
            && format.frame_format != args.format.preferred_frame_format()
        {
            log::warn!(
                "--format={} was requested but the camera negotiated {}; continuing with the negotiated format",
                args.format,
                format.frame_format,
            );
        }
        Ok((format.resolution.width, format.resolution.height, VideoInput::Camera(source)))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!(
            "camera capture is not supported on {}; local_video supports macOS AVFoundation and Linux V4L2",
            std::env::consts::OS
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let ctrl_c_received = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let ctrl_c_received = ctrl_c_received.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            ctrl_c_received.store(true, Ordering::Release);
            info!("Ctrl-C received, exiting...");
        }
    });

    run(args, ctrl_c_received).await
}

async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    let log_range = FrameLogRange::new(args.log_start_frame_id, args.log_end_frame_id)?;
    let logging_enabled = args.log_csv.is_some();
    let attach_timestamp = args.attach_timestamp || logging_enabled;
    let attach_frame_id = args.attach_frame_id || logging_enabled;

    if args.list_cameras {
        return list_cameras();
    }
    if args.list_encoders {
        list_encoders();
        return Ok(());
    }

    // LiveKit connection details
    let url = args
        .url
        .clone()
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LIVEKIT_URL must be provided via --url or env");
    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LIVEKIT_API_KEY must be provided via --api-key or env");
    let api_secret = args
        .api_secret
        .clone()
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LIVEKIT_API_SECRET must be provided via --api-secret or env");

    if let Some((min_playout_delay, max_playout_delay)) =
        requested_playout_delay(args.min_playout_delay, args.max_playout_delay)
    {
        let twirp_host = normalize_twirp_host(&url);
        let room_client = RoomClient::with_api_key(&twirp_host, &api_key, &api_secret);
        info!(
            "Recreating room '{}' with playout delay min={} max={} ms",
            args.room_name, min_playout_delay, max_playout_delay
        );
        match room_client.delete_room(&args.room_name).await {
            Ok(()) => info!("Deleted existing room '{}'", args.room_name),
            Err(err) if is_twirp_not_found(&err) => {
                debug!("Room '{}' did not exist before recreation", args.room_name);
            }
            Err(err) => return Err(err.into()),
        }
        room_client
            .create_room_with_playout_delay(
                &args.room_name,
                CreateRoomOptions::default(),
                min_playout_delay,
                max_playout_delay,
            )
            .await?;
    }

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
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

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.dynacast = args.dynacast;

    // Configure E2EE if an encryption key is provided
    if let Some(ref e2ee_key) = args.e2ee_key {
        let key_provider = KeyProvider::with_shared_key(
            KeyProviderOptions::default(),
            e2ee_key.as_bytes().to_vec(),
        );
        room_options.encryption =
            Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
        info!("E2EE enabled with AES-GCM encryption");
    }

    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = std::sync::Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Enable E2EE after connection
    if args.e2ee_key.is_some() {
        room.e2ee_manager().set_enabled(true);
        info!("End-to-end encryption activated");
    }

    // Log room events
    {
        let room_clone = room.clone();
        tokio::spawn(async move {
            let mut events = room_clone.subscribe();
            info!("Subscribed to room events");
            while let Some(evt) = events.recv().await {
                debug!("Room event: {:?}", evt);
            }
        });
    }

    let (width, height, video_input) = match args.source {
        SourceKind::Argus => {
            #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
            {
                if args.test_pattern.is_some() {
                    anyhow::bail!("--test-pattern is not supported with --source argus");
                }
                if args.display_video {
                    anyhow::bail!("--display-video is not supported with --source argus");
                }
                let session = ArgusCaptureSession::new(ArgusCaptureOptions::new(
                    args.camera_index as u32,
                    CaptureResolution::new(args.width, args.height),
                    args.fps,
                ))?;
                info!(
                    "Argus MIPI capture session opened: {}x{} @ {} fps (camera {})",
                    session.width(),
                    session.height(),
                    args.fps,
                    args.camera_index,
                );
                (session.width(), session.height(), VideoInput::Argus(session))
            }
            #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
            {
                anyhow::bail!(
                    "--source argus requires Linux aarch64 on NVIDIA Jetson; this binary was built for {}-{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                );
            }
        }
        SourceKind::Uvc => {
            if let Some(test_pattern) = args.test_pattern {
                let width = args.width;
                let height = args.height;
                let fps = args.fps;
                info!(
                    "Test pattern enabled: {} at {}x{} @ {} fps",
                    test_pattern.label(),
                    width,
                    height,
                    fps
                );
                (
                    width,
                    height,
                    VideoInput::TestPattern(TestPattern::new(width, height, test_pattern)),
                )
            } else {
                open_camera_source(&args)?
            }
        }
    };
    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));
    let display_shared = args.display_video.then(|| Arc::new(Mutex::new(SharedYuv::default())));
    let publisher_log = args
        .log_csv
        .as_deref()
        .map(|path| PublisherCsvLogger::new(path, log_range))
        .transpose()
        .with_context(|| {
            format!(
                "failed to create publisher frame log at {}",
                args.log_csv.as_deref().expect("log path should be present").display()
            )
        })?;
    if let Some(path) = &args.log_csv {
        info!(
            "Writing publisher per-frame metrics to {} (frame-ID bounds are inclusive)",
            path.display()
        );
    }
    let publish_timing_state = (args.display_timing || logging_enabled).then(|| {
        Arc::new(Mutex::new(PublisherTimingState {
            frame_log: publisher_log,
            ..PublisherTimingState::default()
        }))
    });

    if let Some(timing_state) = publish_timing_state.as_ref() {
        let timing_state = timing_state.clone();
        let display_shared_for_timing = display_shared.clone();
        let mut events = track.publish_timing_events();
        tokio::spawn(async move {
            use tokio_stream::StreamExt;

            while let Some(event) = events.next().await {
                let sample = timing_state.lock().record_sdk_event(event);
                if let Some(sample) = sample {
                    update_shared_timing_sample(display_shared_for_timing.as_ref(), sample);
                }
            }
        });
    }

    // Choose requested codec and attempt to publish; if H.265 fails, retry with H.264
    let requested_codec = VideoCodec::from(args.codec);
    let requested_encoder = VideoEncoderBackend::from(args.encoder);
    let available_encoders: Vec<_> = VideoEncoderBackend::list_available().into_iter().collect();
    info!(
        "Available video encoder backends: {}",
        available_encoders
            .iter()
            .map(|backend| video_encoder_backend_name(*backend))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !available_encoders.contains(&requested_encoder) {
        log::warn!(
            "Requested video encoder backend '{}' is not reported as available; libwebrtc may fall back to another compatible encoder",
            args.encoder.as_str()
        );
    }
    info!(
        "Attempting publish with codec: {}, encoder: {}",
        requested_codec.as_str(),
        args.encoder.as_str()
    );

    // Compute an explicit video encoding so the published layer uses the requested FPS.
    // When simulcast is enabled, lower layers also use this FPS instead of SDK defaults.
    let target_fps = args.fps as f64;
    let main_encoding = {
        let base = options::compute_appropriate_encoding(false, width, height, requested_codec);
        VideoEncoding {
            max_bitrate: args.max_bitrate.unwrap_or(base.max_bitrate),
            max_framerate: target_fps,
        }
    };
    let simulcast_presets = compute_simulcast_presets_30fps(width, height, target_fps);
    if args.simulcast {
        info!(
            "Video encoding: {}x{} @ {:.0} fps, {} bps (simulcast layers: {})",
            width,
            height,
            target_fps,
            main_encoding.max_bitrate,
            simulcast_presets
                .iter()
                .map(|p| format!(
                    "{}x{}@{:.0}fps/{}bps",
                    p.width, p.height, p.encoding.max_framerate, p.encoding.max_bitrate
                ))
                .collect::<Vec<_>>()
                .join(", "),
        );
    } else {
        info!(
            "Video encoding: {}x{} @ {:.0} fps, {} bps (simulcast disabled)",
            width, height, target_fps, main_encoding.max_bitrate,
        );
    }

    let mut frame_metadata_features = FrameMetadataFeatures::default();
    frame_metadata_features.user_timestamp = attach_timestamp;
    frame_metadata_features.frame_id = attach_frame_id;
    frame_metadata_features.user_data = args.attach_user_data;

    let publish_opts = |codec: VideoCodec| TrackPublishOptions {
        source: TrackSource::Camera,
        simulcast: args.simulcast,
        video_codec: codec,
        video_encoder: requested_encoder,
        frame_metadata_features,
        video_encoding: Some(main_encoding.clone()),
        simulcast_layers: args.simulcast.then(|| simulcast_presets.clone()),
        ..Default::default()
    };

    let publish_result = room
        .local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_opts(requested_codec))
        .await;

    let actual_codec = if let Err(e) = publish_result {
        if matches!(requested_codec, VideoCodec::H265) {
            log::warn!("H.265 publish failed ({}). Falling back to H.264...", e);
            room.local_participant()
                .publish_track(LocalTrack::Video(track.clone()), publish_opts(VideoCodec::H264))
                .await?;
            info!("Published camera track with H.264 fallback");
            VideoCodec::H264
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published camera track");
        requested_codec
    };
    let burn_timestamp_requested = attach_timestamp && args.burn_timestamp;
    let zero_copy_supported = publisher_zero_copy_supported(&video_input);
    let zero_copy_active = args.zero_copy && zero_copy_supported;
    if args.zero_copy {
        if let Some(reason) = publisher_zero_copy_unsupported_reason(&video_input) {
            log::warn!("--zero-copy requested, but {reason}; using CPU I420 capture");
        }
    }
    if zero_copy_active && burn_timestamp_requested {
        log::warn!(
            "--zero-copy keeps frames out of CPU memory; --burn-timestamp will not draw an overlay"
        );
    }
    info!(
        "Publisher media path: capture={}, encode=requested codec {} via {}",
        publisher_capture_path_label(&video_input, burn_timestamp_requested, args.zero_copy),
        actual_codec.as_str(),
        video_encoder_backend_name(requested_encoder),
    );
    let zero_copy_fallback =
        publisher_uses_zero_copy_camera_capture(&video_input, zero_copy_active)
            .then(|| Arc::new(AtomicBool::new(false)));

    let capture_config = CaptureConfig {
        fps: args.fps,
        zero_copy: zero_copy_active,
        attach_timestamp,
        burn_timestamp: args.burn_timestamp,
        attach_frame_id,
        display_timing: args.display_timing,
    };

    // Shared keyboard-controlled channel values, written by the preview window
    // and read by the capture loop to fill the user_data trailer.
    let user_data_channels =
        args.attach_user_data.then(|| Arc::new(Mutex::new([0.0f32; user_data::NUM_CHANNELS])));

    let publish_stats_task = tokio::spawn(update_publisher_video_stats(
        track.clone(),
        ctrl_c_received.clone(),
        zero_copy_fallback.clone(),
    ));

    match video_input {
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        VideoInput::Argus(session) => {
            let capture_result = run_argus_capture_loop(
                capture_config,
                ctrl_c_received,
                rtc_source,
                session,
                width,
                height,
                publish_timing_state.clone(),
                user_data_channels.clone(),
            )
            .await;
            let _ = publish_stats_task.await;
            capture_result?;
        }
        video_input => {
            if args.display_video {
                let shared =
                    display_shared.expect("display video should create shared preview state");
                {
                    let mut shared = shared.lock();
                    shared.codec = actual_codec.as_str().to_ascii_uppercase();
                    shared.simulcast = args.simulcast;
                }
                let overlay_task = tokio::spawn(update_publisher_encoder_overlay(
                    track.clone(),
                    shared.clone(),
                    ctrl_c_received.clone(),
                ));
                let capture_task = tokio::spawn(run_capture_loop(
                    capture_config,
                    ctrl_c_received.clone(),
                    track.clone(),
                    rtc_source,
                    video_input,
                    width,
                    height,
                    Some(shared.clone()),
                    publish_timing_state.clone(),
                    user_data_channels.clone(),
                    zero_copy_fallback.clone(),
                ));

                let display_result = video_display::run_display(
                    "LiveKit Video Publisher",
                    shared,
                    ctrl_c_received.clone(),
                    Some(width as f32 / height as f32),
                    user_data_channels.clone(),
                );

                let capture_result = capture_task.await?;
                let _ = publish_stats_task.await;
                let _ = overlay_task.await;
                display_result?;
                capture_result?;
            } else {
                let capture_result = run_capture_loop(
                    capture_config,
                    ctrl_c_received,
                    track,
                    rtc_source,
                    video_input,
                    width,
                    height,
                    None,
                    publish_timing_state.clone(),
                    user_data_channels.clone(),
                    zero_copy_fallback.clone(),
                )
                .await;
                let _ = publish_stats_task.await;
                capture_result?;
            }
        }
    }

    Ok(())
}

/// Maximum number of back-to-back camera capture/convert failures tolerated
/// before the publish is aborted; isolated failures (e.g. one corrupt MJPEG
/// frame) are logged and skipped.
const MAX_CONSECUTIVE_CAPTURE_FAILURES: u32 = 30;

/// Runs the test-pattern/camera capture loop.
///
/// Camera backends block inside [`VideoCaptureSource::next_frame`] until a
/// frame arrives (AVFoundation parks on a condvar), so the loop body runs on a
/// dedicated blocking thread, mirroring [`run_argus_capture_loop`]. A watcher
/// task turns the shutdown flag (Ctrl-C or preview window close) into a
/// [`CaptureStopHandle::stop`] call so a blocked `next_frame` returns promptly
/// instead of hanging the process.
async fn run_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    track: LocalVideoTrack,
    rtc_source: NativeVideoSource,
    video_input: VideoInput,
    width: u32,
    height: u32,
    display_shared: Option<Arc<Mutex<SharedYuv>>>,
    publish_timing_state: Option<Arc<Mutex<PublisherTimingState>>>,
    user_data_channels: Option<Arc<Mutex<[f32; user_data::NUM_CHANNELS]>>>,
    zero_copy_fallback: Option<Arc<AtomicBool>>,
) -> Result<()> {
    let stop_handle = CaptureStopHandle::for_input(&video_input);
    let stop_watcher = tokio::spawn({
        let ctrl_c_received = ctrl_c_received.clone();
        let stop_handle = stop_handle.clone();
        async move {
            while !ctrl_c_received.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            stop_handle.stop();
        }
    });

    let capture_result = tokio::task::spawn_blocking({
        let ctrl_c_received = ctrl_c_received.clone();
        move || {
            run_capture_loop_blocking(
                config,
                ctrl_c_received,
                track,
                rtc_source,
                video_input,
                width,
                height,
                display_shared,
                publish_timing_state,
                user_data_channels,
                zero_copy_fallback,
            )
        }
    })
    .await;
    stop_watcher.abort();
    // Unblock the stats/overlay/display tasks when the loop exits on its own
    // (e.g. after repeated capture failures) rather than via the shutdown flag.
    ctrl_c_received.store(true, Ordering::Release);
    capture_result?
}

fn run_capture_loop_blocking(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    track: LocalVideoTrack,
    rtc_source: NativeVideoSource,
    mut video_input: VideoInput,
    width: u32,
    height: u32,
    display_shared: Option<Arc<Mutex<SharedYuv>>>,
    publish_timing_state: Option<Arc<Mutex<PublisherTimingState>>>,
    user_data_channels: Option<Arc<Mutex<[f32; user_data::NUM_CHANNELS]>>>,
    zero_copy_fallback: Option<Arc<AtomicBool>>,
) -> Result<()> {
    let pace_fps = config.fps as f64;
    #[cfg(target_os = "macos")]
    let camera_driven_pacing =
        matches!(&video_input, VideoInput::Camera(VideoCaptureSource::AvFoundation { .. }));
    #[cfg(not(target_os = "macos"))]
    let camera_driven_pacing = false;
    let target = Duration::from_secs_f64(1.0 / pace_fps);
    // Deadline-based pacing with skipped missed intervals, equivalent to the
    // previous tokio interval with `MissedTickBehavior::Skip`.
    let mut next_frame_deadline = Instant::now() + target;
    let start_ts = Instant::now();

    // Capture loop
    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let mut fps_window_frames: u64 = 0;
    let mut fps_window_start = Instant::now();
    let mut fps_smoothed: f32 = 0.0;
    let burn_timestamp_requested = config.attach_timestamp && config.burn_timestamp;
    info!("Target frame interval: {:.2} ms", target.as_secs_f64() * 1000.0);
    if camera_driven_pacing {
        info!("Capture pacing: camera frame-arrival driven");
    } else {
        info!("Capture pacing: application timer driven");
    }

    // Timing accumulators (ms) for rolling stats
    let mut timings = PublisherTimingSummary::default();
    let mut frame_counter: u32 = 1;
    let mut test_pattern_frame_index: u64 = 0;
    let mut timestamp_overlay =
        burn_timestamp_requested.then(|| TimestampOverlay::new(width, height));
    let align_buffers_for_display = display_shared.is_some();
    let mut logged_camera_timestamp_source = false;
    let mut logged_camera_timestamp_fallback = false;
    let mut logged_zero_copy_fallback = false;
    let mut consecutive_capture_failures: u32 = 0;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        let paced_wait_started_at = Instant::now();
        if !camera_driven_pacing {
            if let Some(wait) = next_frame_deadline.checked_duration_since(paced_wait_started_at) {
                std::thread::sleep(wait);
            }
            let now = Instant::now();
            next_frame_deadline += target;
            while next_frame_deadline <= now {
                next_frame_deadline += target;
            }
        }
        let paced_wait_finished_at = Instant::now();

        let source_frame_read_started_at = Instant::now();
        let mut sourced = match &mut video_input {
            VideoInput::TestPattern(pattern) => {
                let frame_wall_time_us = unix_time_us_now();
                // WebRTC may queue the frame and hardware encoders may upload it asynchronously.
                // Give each submitted frame unique backing storage so later captures cannot
                // overwrite buffers that are still in-flight.
                let mut frame = VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: 0,
                    frame_metadata: None,
                    buffer: create_i420_buffer(width, height, align_buffers_for_display),
                };
                let (stride_y, stride_u, stride_v) = frame.buffer.strides();
                let (data_y, data_u, data_v) = frame.buffer.data_mut();
                pattern.render(
                    data_y,
                    stride_y as i32,
                    data_u,
                    stride_u as i32,
                    data_v,
                    stride_v as i32,
                    test_pattern_frame_index,
                );
                test_pattern_frame_index = test_pattern_frame_index.wrapping_add(1);
                let frame_acquired_at = Instant::now();
                SourcedFrame {
                    buffer: CapturedFrameBuffer::I420(frame),
                    capture_wall_time_us: frame_wall_time_us,
                    read_wall_time_us: unix_time_us_now(),
                    acquired_at: frame_acquired_at,
                    pipeline_started_at: source_frame_read_started_at,
                    has_camera_timestamp: false,
                }
            }
            VideoInput::Camera(source) => {
                let force_raw_after_zero_copy_failure = zero_copy_fallback
                    .as_ref()
                    .is_some_and(|fallback| fallback.load(Ordering::Acquire));
                if force_raw_after_zero_copy_failure && !logged_zero_copy_fallback {
                    log::warn!(
                        "Publisher media path changed: capture=AVFoundation CPU I420 fallback after zero-copy encode starvation"
                    );
                    logged_zero_copy_fallback = true;
                    // Switch the facade to CPU-accessible frames for the rest of the run.
                    #[cfg(target_os = "macos")]
                    if let VideoCaptureSource::AvFoundation { prefer_raw_frames, .. } = source {
                        *prefer_raw_frames = true;
                    }
                }
                let captured = match source.next_frame() {
                    Ok(frame) => {
                        consecutive_capture_failures = 0;
                        frame
                    }
                    Err(err) => {
                        if ctrl_c_received.load(Ordering::Acquire) {
                            // `stop()` interrupted a blocked `next_frame` during shutdown.
                            break;
                        }
                        consecutive_capture_failures += 1;
                        log::warn!(
                            "Camera frame capture failed ({consecutive_capture_failures} consecutive): {err}"
                        );
                        if consecutive_capture_failures >= MAX_CONSECUTIVE_CAPTURE_FAILURES {
                            return Err(anyhow::Error::new(err).context(format!(
                                "camera capture failed {MAX_CONSECUTIVE_CAPTURE_FAILURES} times in a row"
                            )));
                        }
                        std::thread::sleep(Duration::from_millis(
                            5 * u64::from(consecutive_capture_failures.min(20)),
                        ));
                        continue;
                    }
                };
                let mut sourced = sourced_frame_from_capture(captured)?;
                match &mut sourced.buffer {
                    CapturedFrameBuffer::I420(frame) => {
                        frame.rotation = VideoRotation::VideoRotation0;
                    }
                    #[cfg(target_os = "macos")]
                    CapturedFrameBuffer::Native(frame) => {
                        frame.rotation = VideoRotation::VideoRotation0;
                    }
                }
                if sourced.has_camera_timestamp {
                    if !logged_camera_timestamp_source {
                        let capture_timestamp_age_ms =
                            sourced.read_wall_time_us.saturating_sub(sourced.capture_wall_time_us)
                                as f64
                                / 1000.0;
                        info!(
                            "Using camera-provided capture timestamp (age at frame read {:.2} ms)",
                            capture_timestamp_age_ms
                        );
                        logged_camera_timestamp_source = true;
                    }
                } else if !logged_camera_timestamp_fallback {
                    log::warn!(
                        "Camera-provided capture timestamp unavailable or implausible; using frame read wall clock"
                    );
                    logged_camera_timestamp_fallback = true;
                }

                sourced
            }
            #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
            VideoInput::Argus(_) => {
                // The Argus source bypasses this loop entirely and is dispatched to
                // `run_argus_capture_loop` from `run`. This arm exists only to satisfy
                // exhaustiveness checking on Jetson builds.
                unreachable!("argus video input must be driven by run_argus_capture_loop")
            }
        };

        let fid = if config.attach_frame_id {
            let id = frame_counter;
            frame_counter = frame_counter.wrapping_add(1);
            Some(id)
        } else {
            None
        };
        if let Some(timing_state) = publish_timing_state.as_ref() {
            timing_state.lock().record_frame_buffer(
                sourced.capture_wall_time_us,
                sourced.read_wall_time_us,
                fid,
            );
        }
        let mut buffer_ready_at = sourced.acquired_at;
        let mut frame_draw_ms = None;
        let mut burned_timestamp_us = None;
        let frame_uses_zero_copy = match &sourced.buffer {
            #[cfg(target_os = "macos")]
            CapturedFrameBuffer::Native(_) => true,
            _ => false,
        };
        if !frame_uses_zero_copy {
            if let Some(overlay) = timestamp_overlay.as_mut() {
                let overlay_started_at = Instant::now();
                match &mut sourced.buffer {
                    CapturedFrameBuffer::I420(frame) => {
                        let (stride_y, _, _) = frame.buffer.strides();
                        let (data_y, _, _) = frame.buffer.data_mut();
                        overlay.draw(data_y, stride_y as usize, sourced.capture_wall_time_us, fid);
                    }
                    #[cfg(target_os = "macos")]
                    CapturedFrameBuffer::Native(_) => {
                        unreachable!("native frame was classified as zero-copy");
                    }
                }
                burned_timestamp_us = Some(sourced.capture_wall_time_us);
                let overlay_finished_at = Instant::now();
                frame_draw_ms =
                    Some((overlay_finished_at - overlay_started_at).as_secs_f64() * 1000.0);
                buffer_ready_at = overlay_finished_at;
            }
        }

        // Build frame metadata from enabled packet trailer features and local timing correlation.
        let user_ts = if config.attach_timestamp || config.display_timing {
            Some(sourced.capture_wall_time_us)
        } else {
            None
        };
        if burned_timestamp_us.is_some() {
            debug_assert_eq!(burned_timestamp_us, Some(sourced.capture_wall_time_us));
        }
        let user_data =
            user_data_channels.as_ref().map(|targets| user_data::encode(&targets.lock()));
        let frame_metadata = if user_ts.is_some() || fid.is_some() || user_data.is_some() {
            Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid, user_data })
        } else {
            None
        };
        // Monotonic, microseconds since start.
        let timestamp_us = start_ts.elapsed().as_micros() as i64;
        match &mut sourced.buffer {
            CapturedFrameBuffer::I420(frame) => {
                frame.frame_metadata = frame_metadata;
                frame.timestamp_us = timestamp_us;
                rtc_source.capture_frame(frame);
            }
            #[cfg(target_os = "macos")]
            CapturedFrameBuffer::Native(frame) => {
                frame.frame_metadata = frame_metadata;
                frame.timestamp_us = timestamp_us;
                rtc_source.capture_frame(frame);
            }
        }
        let webrtc_capture_finished_at = Instant::now();
        let webrtc_capture_finished_wall_time_us = unix_time_us_now();
        if let Some(shared) = display_shared.as_ref() {
            let timing_sample = if config.display_timing {
                publish_timing_state
                    .as_ref()
                    .and_then(|timing_state| timing_state.lock().display_sample())
            } else {
                None
            };
            match &sourced.buffer {
                CapturedFrameBuffer::I420(frame) => {
                    let (stride_y, stride_u, stride_v) = frame.buffer.strides();
                    let (data_y, data_u, data_v) = frame.buffer.data();
                    video_display::pack_i420_into_shared(
                        shared,
                        width,
                        height,
                        data_y,
                        stride_y as u32,
                        data_u,
                        stride_u as u32,
                        data_v,
                        stride_v as u32,
                        timing_sample,
                    );
                }
                #[cfg(target_os = "macos")]
                CapturedFrameBuffer::Native(frame) => {
                    let i420 = frame.buffer.to_i420();
                    let (stride_y, stride_u, stride_v) = i420.strides();
                    let (data_y, data_u, data_v) = i420.data();
                    video_display::pack_i420_into_shared(
                        shared,
                        width,
                        height,
                        data_y,
                        stride_y as u32,
                        data_u,
                        stride_u as u32,
                        data_v,
                        stride_v as u32,
                        timing_sample,
                    );
                }
            }
        }

        frames += 1;
        fps_window_frames += 1;
        let win_elapsed = fps_window_start.elapsed();
        if win_elapsed >= Duration::from_millis(500) {
            let inst_fps = fps_window_frames as f32 / win_elapsed.as_secs_f32().max(0.001);
            fps_smoothed = if fps_smoothed <= 0.0 {
                inst_fps
            } else {
                (fps_smoothed * 0.7) + (inst_fps * 0.3)
            };
            if let Some(shared) = display_shared.as_ref() {
                shared.lock().fps = fps_smoothed;
            }
            fps_window_frames = 0;
            fps_window_start = Instant::now();
        }

        // Per-iteration timing bookkeeping
        timings
            .paced_wait_ms
            .record((paced_wait_finished_at - paced_wait_started_at).as_secs_f64() * 1000.0);
        timings
            .camera_frame_read_ms
            .record((sourced.acquired_at - source_frame_read_started_at).as_secs_f64() * 1000.0);
        if sourced.has_camera_timestamp && sourced.read_wall_time_us >= sourced.capture_wall_time_us
        {
            timings
                .capture_timestamp_age_ms
                .record((sourced.read_wall_time_us - sourced.capture_wall_time_us) as f64 / 1000.0);
        }
        if sourced.has_camera_timestamp
            && webrtc_capture_finished_wall_time_us >= sourced.capture_wall_time_us
        {
            timings.capture_timestamp_to_webrtc_ms.record(
                (webrtc_capture_finished_wall_time_us - sourced.capture_wall_time_us) as f64
                    / 1000.0,
            );
        }
        if let Some(frame_draw_ms) = frame_draw_ms {
            timings.frame_draw_ms.record(frame_draw_ms);
        }
        timings
            .submit_to_webrtc_ms
            .record((webrtc_capture_finished_at - buffer_ready_at).as_secs_f64() * 1000.0);
        timings.capture_to_webrtc_total_ms.record(
            (webrtc_capture_finished_at - sourced.pipeline_started_at).as_secs_f64() * 1000.0,
        );

        if last_fps_log.elapsed() >= std::time::Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let layers = track.publishing_layers();
            let layers_str = if layers.is_empty() {
                "n/a".to_string()
            } else {
                layers
                    .iter()
                    .map(|layer| {
                        format!(
                            "{}({})={}",
                            layer.rid,
                            layer.quality,
                            if layer.active { "on" } else { "off" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            info!(
                "Video status: {}x{} | ~{:.1} fps | layers: [{}] | target {:.2} ms",
                width,
                height,
                fps_est,
                layers_str,
                target.as_secs_f64() * 1000.0,
            );
            info!("{}", format_timing_line(&timings));
            frames = 0;
            timings.reset();
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

/// Capture loop dedicated to Jetson MIPI capture via libargus.
///
/// Argus blocks inside `acquireFrame`, pacing capture itself, so this loop runs in a
/// dedicated OS thread. With `--zero-copy`, the path pushes NV12 DMA-buffer fds
/// straight into [`NativeVideoSource::capture_dmabuf_frame_with_metadata`] for
/// hand-off to the Jetson hardware encoder; otherwise it copies to CPU I420.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
async fn run_argus_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    session: ArgusCaptureSession,
    width: u32,
    height: u32,
    publish_timing_state: Option<Arc<Mutex<PublisherTimingState>>>,
    user_data_channels: Option<Arc<Mutex<[f32; user_data::NUM_CHANNELS]>>>,
) -> Result<()> {
    let capture_handle = std::thread::Builder::new()
        .name("mipi-capture".into())
        .spawn(move || -> Result<()> {
            enum CapturedArgusFrame {
                DmaBuf(argus::ArgusFrame),
                I420(argus::ArgusI420Frame),
            }

            let mut session = session;
            let burn_timestamp_requested = config.attach_timestamp && config.burn_timestamp;
            let burn_timestamp_active = burn_timestamp_requested && !config.zero_copy;
            let mut timestamp_overlay =
                burn_timestamp_active.then(|| TimestampOverlay::new(width, height));
            let mut frames: u64 = 0;
            let mut last_fps_log = Instant::now();
            let mut sum_acquire_ms = 0.0;
            let mut sum_argus_wait_ms = 0.0;
            let mut sum_argus_blit_ms = 0.0;
            let mut sum_argus_i420_copy_ms = 0.0;
            let mut sum_timestamp_burn_ms = 0.0;
            let mut sum_capture_ms = 0.0;
            let mut sum_iter_ms = 0.0;
            let mut consecutive_failures: u32 = 0;
            let mut frame_counter: u32 = 1;
            let mut logged_sensor_ts_source = false;
            let mut logged_sensor_ts_missing = false;
            let mut logged_sensor_ts_conversion_failed = false;
            let mut sensor_timestamp_frames: u64 = 0;
            let mut backup_timestamp_frames: u64 = 0;
            let mut sum_sensor_to_acquire_ms = 0.0;
            let mut sum_sensor_to_argus_acquire_ms = 0.0;
            if burn_timestamp_active {
                info!(
                    "Argus timestamp burn enabled: copying NV12 DMA-BUF frames to CPU I420 before publish"
                );
            }

            loop {
                if ctrl_c_received.load(Ordering::Acquire) {
                    break;
                }

                let iter_start = Instant::now();
                let acquire_started_at = Instant::now();
                let capture_result = if config.zero_copy {
                    session.capture_frame().map(CapturedArgusFrame::DmaBuf)
                } else {
                    session.capture_i420_frame().map(CapturedArgusFrame::I420)
                };
                let captured_frame = match capture_result {
                    Ok(frame) => {
                        consecutive_failures = 0;
                        frame
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        if consecutive_failures <= 3 {
                            log::warn!(
                                "MIPI frame acquisition failed (attempt {}): {}",
                                consecutive_failures,
                                e
                            );
                        }
                        let backoff =
                            Duration::from_millis(5 * (consecutive_failures as u64).min(20));
                        std::thread::sleep(backoff);
                        continue;
                    }
                };
                let acquire_finished_at = Instant::now();
                let argus_frame = match &captured_frame {
                    CapturedArgusFrame::DmaBuf(frame) => frame,
                    CapturedArgusFrame::I420(frame) => &frame.dmabuf,
                };
                let argus_wait_ms = argus_frame.acquire_wait_ns as f64 / 1_000_000.0;
                let argus_blit_ms = argus_frame.blit_ns as f64 / 1_000_000.0;
                let fallback_wall_time_us =
                    if config.attach_timestamp { unix_time_us_now() } else { 0 };

                let (capture_wall_time_us, timestamp_from_sensor) = if config.attach_timestamp {
                    match argus_frame.sensor_timestamp_ns {
                        Some(sensor_timestamp_ns) => match argus::sensor_monotonic_ns_to_unix_us(
                            sensor_timestamp_ns,
                            fallback_wall_time_us,
                        ) {
                            Some(sensor_wall_time_us) => {
                                if !logged_sensor_ts_source {
                                    info!(
                                        "Using Argus sensor timestamp for packet trailer user_timestamp"
                                    );
                                    logged_sensor_ts_source = true;
                                }
                                (sensor_wall_time_us, true)
                            }
                            None => {
                                if !logged_sensor_ts_conversion_failed {
                                    log::warn!(
                                        "Failed to convert Argus sensor timestamp to wall time; using backup system wall clock for packet trailer user_timestamp"
                                    );
                                    logged_sensor_ts_conversion_failed = true;
                                }
                                (fallback_wall_time_us, false)
                            }
                        },
                        None => {
                            if !logged_sensor_ts_missing {
                                log::warn!(
                                    "Argus sensor timestamp not available; using backup system wall clock for packet trailer user_timestamp"
                                );
                                logged_sensor_ts_missing = true;
                            }
                            (fallback_wall_time_us, false)
                        }
                    }
                } else {
                    (0, false)
                };
                if config.attach_timestamp {
                    if timestamp_from_sensor {
                        sensor_timestamp_frames += 1;
                        let sensor_to_acquire_ms =
                            fallback_wall_time_us.saturating_sub(capture_wall_time_us) as f64
                                / 1_000.0;
                        sum_sensor_to_acquire_ms += sensor_to_acquire_ms;
                        sum_sensor_to_argus_acquire_ms +=
                            (sensor_to_acquire_ms - argus_blit_ms).max(0.0);
                    } else {
                        backup_timestamp_frames += 1;
                    }
                }
                let user_ts =
                    if config.attach_timestamp { Some(capture_wall_time_us) } else { None };
                let fid = if config.attach_frame_id {
                    let id = frame_counter;
                    frame_counter = frame_counter.wrapping_add(1);
                    Some(id)
                } else {
                    None
                };
                if let Some(timing_state) = publish_timing_state.as_ref() {
                    timing_state.lock().record_frame_buffer(
                        capture_wall_time_us,
                        fallback_wall_time_us,
                        fid,
                    );
                }
                let user_data =
                    user_data_channels.as_ref().map(|targets| user_data::encode(&targets.lock()));
                let frame_metadata = if user_ts.is_some() || fid.is_some() || user_data.is_some() {
                    Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid, user_data })
                } else {
                    None
                };

                match captured_frame {
                    CapturedArgusFrame::DmaBuf(argus_frame) => {
                        let plane = argus_frame
                            .dmabuf
                            .planes
                            .first()
                            .ok_or_else(|| anyhow::anyhow!("Argus DMA-BUF frame missing plane"))?;
                        rtc_source.capture_dmabuf_frame_with_metadata(
                            plane.fd,
                            argus_frame.dmabuf.width,
                            argus_frame.dmabuf.height,
                            0, // NV12
                            argus_frame.dmabuf.timestamp_us,
                            frame_metadata,
                        );
                    }
                    CapturedArgusFrame::I420(mut argus_i420_frame) => {
                        if let Some(overlay) = timestamp_overlay.as_mut() {
                            let overlay_started_at = Instant::now();
                            let (stride_y, _, _) = argus_i420_frame.frame.buffer.strides();
                            let (data_y, _, _) = argus_i420_frame.frame.buffer.data_mut();
                            overlay.draw(data_y, stride_y as usize, capture_wall_time_us, fid);
                            sum_timestamp_burn_ms +=
                                overlay_started_at.elapsed().as_secs_f64() * 1000.0;
                        }
                        sum_argus_i420_copy_ms +=
                            argus_i420_frame.copy_to_i420_ns as f64 / 1_000_000.0;
                        argus_i420_frame.frame.frame_metadata = frame_metadata;
                        argus_i420_frame.frame.timestamp_us =
                            argus_i420_frame.dmabuf.dmabuf.timestamp_us;
                        rtc_source.capture_frame(&argus_i420_frame.frame);
                    }
                }
                let capture_finished_at = Instant::now();

                frames += 1;
                sum_acquire_ms += (acquire_finished_at - acquire_started_at).as_secs_f64() * 1000.0;
                sum_argus_wait_ms += argus_wait_ms;
                sum_argus_blit_ms += argus_blit_ms;
                sum_capture_ms +=
                    (capture_finished_at - acquire_finished_at).as_secs_f64() * 1000.0;
                sum_iter_ms += (Instant::now() - iter_start).as_secs_f64() * 1000.0;

                if last_fps_log.elapsed() >= Duration::from_secs(2) {
                    let secs = last_fps_log.elapsed().as_secs_f64();
                    let fps_est = frames as f64 / secs;
                    let n = frames.max(1) as f64;
                    if config.attach_timestamp {
                        let sensor_age_ms = if sensor_timestamp_frames > 0 {
                            sum_sensor_to_acquire_ms / sensor_timestamp_frames as f64
                        } else {
                            0.0
                        };
                        let sensor_to_argus_acquire_ms = if sensor_timestamp_frames > 0 {
                            sum_sensor_to_argus_acquire_ms / sensor_timestamp_frames as f64
                        } else {
                            0.0
                        };
                        if burn_timestamp_active {
                            info!(
                                "MIPI publishing: {}x{}, ~{:.1} fps | packet trailer timestamp source: sensor {} frames, backup system {} frames | avg ms: sensor_to_argus_acquire {:.2}, argus_wait {:.2}, argus_blit {:.2}, argus_i420_copy {:.2}, timestamp_burn {:.2}, sensor_to_acquire {:.2}, acquire {:.2}, capture {:.2}, iter {:.2}",
                                width,
                                height,
                                fps_est,
                                sensor_timestamp_frames,
                                backup_timestamp_frames,
                                sensor_to_argus_acquire_ms,
                                sum_argus_wait_ms / n,
                                sum_argus_blit_ms / n,
                                sum_argus_i420_copy_ms / n,
                                sum_timestamp_burn_ms / n,
                                sensor_age_ms,
                                sum_acquire_ms / n,
                                sum_capture_ms / n,
                                sum_iter_ms / n,
                            );
                        } else {
                            info!(
                                "MIPI publishing: {}x{}, ~{:.1} fps | packet trailer timestamp source: sensor {} frames, backup system {} frames | avg ms: sensor_to_argus_acquire {:.2}, argus_wait {:.2}, argus_blit {:.2}, sensor_to_acquire {:.2}, acquire {:.2}, capture {:.2}, iter {:.2}",
                                width,
                                height,
                                fps_est,
                                sensor_timestamp_frames,
                                backup_timestamp_frames,
                                sensor_to_argus_acquire_ms,
                                sum_argus_wait_ms / n,
                                sum_argus_blit_ms / n,
                                sensor_age_ms,
                                sum_acquire_ms / n,
                                sum_capture_ms / n,
                                sum_iter_ms / n,
                            );
                        }
                    } else {
                        info!(
                            "MIPI publishing: {}x{}, ~{:.1} fps | packet trailer timestamp: disabled | avg ms: argus_wait {:.2}, argus_blit {:.2}, acquire {:.2}, capture {:.2}, iter {:.2}",
                            width,
                            height,
                            fps_est,
                            sum_argus_wait_ms / n,
                            sum_argus_blit_ms / n,
                            sum_acquire_ms / n,
                            sum_capture_ms / n,
                            sum_iter_ms / n,
                        );
                    }
                    frames = 0;
                    sensor_timestamp_frames = 0;
                    backup_timestamp_frames = 0;
                    sum_acquire_ms = 0.0;
                    sum_argus_wait_ms = 0.0;
                    sum_argus_blit_ms = 0.0;
                    sum_argus_i420_copy_ms = 0.0;
                    sum_timestamp_burn_ms = 0.0;
                    sum_capture_ms = 0.0;
                    sum_iter_ms = 0.0;
                    sum_sensor_to_acquire_ms = 0.0;
                    sum_sensor_to_argus_acquire_ms = 0.0;
                    last_fps_log = Instant::now();
                }
            }

            Ok(())
        })?;

    tokio::task::spawn_blocking(move || {
        capture_handle
            .join()
            .map_err(|e| anyhow::anyhow!("MIPI capture thread panicked: {:?}", e))?
    })
    .await??;

    Ok(())
}

/// Build simulcast presets that match the SDK defaults but with a uniform frame rate.
/// The SDK's built-in `DEFAULT_SIMULCAST_PRESETS` use 15/20 fps for lower layers;
/// this keeps the same resolutions and bitrates but overrides fps to `target_fps`.
fn compute_simulcast_presets_30fps(width: u32, height: u32, target_fps: f64) -> Vec<VideoPreset> {
    let ar = width as f32 / height as f32;
    let defaults: &[VideoPreset] = if f32::abs(ar - 16.0 / 9.0) < f32::abs(ar - 4.0 / 3.0) {
        video_presets::DEFAULT_SIMULCAST_PRESETS
    } else {
        livekit::options::video43::DEFAULT_SIMULCAST_PRESETS
    };
    defaults
        .iter()
        .map(|p| VideoPreset::new(p.width, p.height, p.encoding.max_bitrate, target_fps))
        .collect()
}
