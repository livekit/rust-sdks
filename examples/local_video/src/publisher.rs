use anyhow::Result;
use clap::{Parser, ValueEnum};
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{
    self, video as video_presets, FrameMetadataFeatures, TrackPublishOptions, VideoCodec,
    VideoEncoderBackend, VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
#[cfg(target_os = "linux")]
use livekit::webrtc::video_frame::NV12Buffer;
use livekit::webrtc::video_frame::{FrameMetadata, I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use livekit_api::services::room::{CreateRoomOptions, RoomClient};
use livekit_api::services::{ServiceError, TwirpError, TwirpErrorCode};
use log::{debug, info};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
    Resolution,
};
use nokhwa::Camera;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use yuv_sys;

#[cfg(target_os = "linux")]
use v4l::buffer::Type as BufType;
#[cfg(target_os = "linux")]
use v4l::io::traits::CaptureStream;
#[cfg(target_os = "linux")]
use v4l::prelude::*;
#[cfg(target_os = "linux")]
use v4l::v4l_sys::*;
#[cfg(target_os = "linux")]
use v4l::video::Capture;
#[cfg(target_os = "linux")]
use v4l::{v4l2, FourCC};

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod argus;
mod codec_display;
mod test_pattern;
mod timestamp_burn;
mod video_display;
mod viewport_aspect;

use test_pattern::TestPattern;
use timestamp_burn::TimestampOverlay;
use video_display::{align_up, PublisherTimingSample, SharedYuv};

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
    /// USB / V4L2 camera via the `nokhwa` crate (default).
    Uvc,
    /// Direct Linux V4L2 mmap capture, including Rockchip ISP multiplanar NV12.
    V4l2,
    /// NVIDIA Jetson MIPI CSI camera via libargus (Jetson-only).
    Argus,
}

/// Selects the camera capture pixel format.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum CaptureFormat {
    /// Try YUYV first and fall back to MJPEG.
    Auto,
    /// Request NV12 direct V4L2 capture.
    Nv12,
    /// Request uncompressed YUYV capture.
    Yuv,
    /// Request compressed MJPEG capture.
    Mjpeg,
}

impl CaptureFormat {
    fn frame_formats(self) -> &'static [FrameFormat] {
        match self {
            Self::Auto => &[FrameFormat::YUYV, FrameFormat::MJPEG],
            Self::Nv12 => &[],
            Self::Yuv => &[FrameFormat::YUYV],
            Self::Mjpeg => &[FrameFormat::MJPEG],
        }
    }
}

impl std::fmt::Display for CaptureFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Nv12 => write!(f, "nv12"),
            Self::Yuv => write!(f, "yuv"),
            Self::Mjpeg => write!(f, "mjpeg"),
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

    /// Camera backend: `uvc` (default, V4L2/USB via nokhwa), `v4l2` (direct Linux V4L2), or `argus` (Jetson MIPI CSI).
    #[arg(long, value_enum, default_value_t = SourceKind::Uvc)]
    source: SourceKind,

    /// Camera capture format: `auto` tries YUYV then MJPEG for UVC; direct V4L2 treats `auto` as NV12.
    #[arg(long, value_enum, default_value_t = CaptureFormat::Auto)]
    format: CaptureFormat,

    /// V4L2 device path for direct Linux capture, e.g. /dev/video-camera0.
    #[arg(long)]
    device: Option<String>,

    /// Generate a standard SMPTE color-bar test pattern instead of using a camera
    #[arg(long, default_value_t = false, conflicts_with_all = ["list_cameras", "list_encoders"])]
    test_pattern: bool,

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

    /// Open a window that displays the video frames being published
    #[arg(long, default_value_t = false)]
    display_video: bool,

    /// Burn publisher timing metrics into the local preview window
    #[arg(long, default_value_t = false, requires = "display_video")]
    display_timing: bool,

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
}

const MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US: u64 = 5_000_000;

#[derive(Default)]
struct CaptureTimestampLogState {
    logged_source: bool,
    logged_missing: bool,
    logged_invalid: bool,
}

fn validate_backend_capture_timestamp_us(
    capture_timestamp: Duration,
    read_wall_time_us: u64,
) -> Result<u64, &'static str> {
    let capture_timestamp_us =
        u64::try_from(capture_timestamp.as_micros()).map_err(|_| "overflows u64")?;
    if capture_timestamp_us == 0 {
        return Err("is zero");
    }
    if capture_timestamp_us > read_wall_time_us {
        return Err("is in the future");
    }
    if read_wall_time_us - capture_timestamp_us > MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US {
        return Err("is too old");
    }
    Ok(capture_timestamp_us)
}

fn select_capture_wall_time_us(
    backend_capture_timestamp: Option<Duration>,
    fallback_wall_time_us: u64,
    read_wall_time_us: u64,
    log_state: &mut CaptureTimestampLogState,
) -> u64 {
    match backend_capture_timestamp {
        Some(capture_timestamp) => {
            match validate_backend_capture_timestamp_us(capture_timestamp, read_wall_time_us) {
                Ok(capture_timestamp_us) => {
                    if !log_state.logged_source {
                        info!("Using camera capture_timestamp for user_timestamp");
                        log_state.logged_source = true;
                    }
                    capture_timestamp_us
                }
                Err(reason) => {
                    if !log_state.logged_invalid {
                        log::warn!(
                            "Ignoring camera capture_timestamp because it {reason}; falling back to system wall clock"
                        );
                        log_state.logged_invalid = true;
                    }
                    fallback_wall_time_us
                }
            }
        }
        None => {
            if !log_state.logged_missing {
                log::warn!(
                    "Buffer::capture_timestamp() not available; falling back to system wall clock"
                );
                log_state.logged_missing = true;
            }
            fallback_wall_time_us
        }
    }
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

fn normalize_twirp_host(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("wss://") {
        return format!("https://{}", rest.trim_end_matches("/rtc"));
    }
    if let Some(rest) = url.strip_prefix("ws://") {
        return format!("http://{}", rest.trim_end_matches("/rtc"));
    }
    url.trim_end_matches("/rtc").to_string()
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
    decode_mjpeg_ms: RollingMs,
    buffer_convert_ms: RollingMs,
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

async fn update_publisher_video_stats(track: LocalVideoTrack, ctrl_c_received: Arc<AtomicBool>) {
    let mut last_log =
        Instant::now().checked_sub(Duration::from_secs(2)).unwrap_or_else(Instant::now);
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        if let Ok(stats) = track.get_stats().await {
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
    let mut last_implementation = String::new();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        match track.get_stats().await {
            Ok(stats) => {
                if let Some(implementation) = find_video_outbound_encoder(&stats) {
                    if implementation != last_implementation {
                        info!("Publisher video encoder implementation: {implementation}");
                        last_implementation = implementation.to_string();
                    }

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
        self.decode_mjpeg_ms.reset();
        self.buffer_convert_ms.reset();
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
    ];
    let mut line_two = Vec::new();

    if let Some(decode_ms) = timings.decode_mjpeg_ms.average() {
        line_two.push(format!("decode_mjpeg {:.2}", decode_ms));
    }

    line_two.push(format!(
        "convert_to_i420 {:.2}",
        timings.buffer_convert_ms.average().unwrap_or_default()
    ));
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

#[derive(Default)]
struct PublisherTimingState {
    samples: HashMap<u64, PublisherTimingSample>,
    order: VecDeque<u64>,
    latest_complete_sample: Option<PublisherTimingSample>,
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
    fn capture_timestamp_validation_rejects_future_timestamp() {
        assert_eq!(
            validate_backend_capture_timestamp_us(Duration::from_micros(1_001), 1_000),
            Err("is in the future")
        );
    }

    #[test]
    fn capture_timestamp_selection_falls_back_for_invalid_backend_timestamp() {
        let mut log_state = CaptureTimestampLogState::default();

        let selected = select_capture_wall_time_us(
            Some(Duration::from_micros(1_001)),
            900,
            1_000,
            &mut log_state,
        );

        assert_eq!(selected, 900);
    }

    #[test]
    fn capture_timestamp_selection_uses_valid_backend_timestamp() {
        let mut log_state = CaptureTimestampLogState::default();

        let selected = select_capture_wall_time_us(
            Some(Duration::from_micros(950)),
            900,
            1_000,
            &mut log_state,
        );

        assert_eq!(selected, 950);
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V4l2CaptureFormat {
    Nv12,
    Yuyv,
    Mjpeg,
}

#[cfg(target_os = "linux")]
impl V4l2CaptureFormat {
    fn from_capture_format(format: CaptureFormat) -> Self {
        match format {
            CaptureFormat::Auto | CaptureFormat::Nv12 => Self::Nv12,
            CaptureFormat::Yuv => Self::Yuyv,
            CaptureFormat::Mjpeg => Self::Mjpeg,
        }
    }

    fn fourcc(self) -> FourCC {
        match self {
            Self::Nv12 => FourCC::new(b"NV12"),
            Self::Yuyv => FourCC::new(b"YUYV"),
            Self::Mjpeg => FourCC::new(b"MJPG"),
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
struct V4l2CaptureConfig {
    device_path: String,
    format: V4l2CaptureFormat,
    width: u32,
    height: u32,
    stride: u32,
    use_mplane: bool,
}

/// Negotiated multiplanar format info returned by raw V4L2 helpers.
#[cfg(target_os = "linux")]
struct MplaneFormat {
    width: u32,
    height: u32,
    fourcc: FourCC,
    stride: u32,
}

/// Multiplanar mmap capture stream.
///
/// The v4l crate's mmap stream does not populate `v4l2_buffer.m.planes` for
/// QUERYBUF/QBUF/DQBUF, which Rockchip ISP mplane devices require.
#[cfg(target_os = "linux")]
struct MplaneStream {
    fd: std::os::unix::io::RawFd,
    bufs: Vec<(*mut u8, usize)>,
    buf_count: u32,
    active: bool,
}

#[cfg(target_os = "linux")]
impl MplaneStream {
    fn new(dev: &Device, buf_count: u32) -> std::io::Result<Self> {
        let fd = dev.handle().fd();

        unsafe {
            let mut reqbufs: v4l2_requestbuffers = std::mem::zeroed();
            reqbufs.count = buf_count;
            reqbufs.type_ = BufType::VideoCaptureMplane as u32;
            reqbufs.memory = v4l::memory::Memory::Mmap as u32;
            v4l2::ioctl(
                fd,
                v4l2::vidioc::VIDIOC_REQBUFS,
                &mut reqbufs as *mut _ as *mut std::os::raw::c_void,
            )?;

            let count = reqbufs.count;
            let mut bufs = Vec::with_capacity(count as usize);
            for i in 0..count {
                let mut plane: v4l2_plane = std::mem::zeroed();
                let mut buf: v4l2_buffer = std::mem::zeroed();
                buf.type_ = BufType::VideoCaptureMplane as u32;
                buf.memory = v4l::memory::Memory::Mmap as u32;
                buf.index = i;
                buf.length = 1;
                buf.m.planes = &mut plane as *mut _;

                v4l2::ioctl(
                    fd,
                    v4l2::vidioc::VIDIOC_QUERYBUF,
                    &mut buf as *mut _ as *mut std::os::raw::c_void,
                )?;

                let len = plane.length as usize;
                let offset = plane.m.mem_offset as libc::off_t;
                let ptr = v4l2::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    offset,
                )?;
                bufs.push((ptr as *mut u8, len));
            }

            Ok(Self { fd, bufs, buf_count: count, active: false })
        }
    }

    fn start(&mut self) -> std::io::Result<()> {
        unsafe {
            for i in 0..self.buf_count {
                let mut plane: v4l2_plane = std::mem::zeroed();
                let mut buf: v4l2_buffer = std::mem::zeroed();
                buf.type_ = BufType::VideoCaptureMplane as u32;
                buf.memory = v4l::memory::Memory::Mmap as u32;
                buf.index = i;
                buf.length = 1;
                buf.m.planes = &mut plane as *mut _;
                v4l2::ioctl(
                    self.fd,
                    v4l2::vidioc::VIDIOC_QBUF,
                    &mut buf as *mut _ as *mut std::os::raw::c_void,
                )?;
            }
            let mut typ = BufType::VideoCaptureMplane as u32;
            v4l2::ioctl(
                self.fd,
                v4l2::vidioc::VIDIOC_STREAMON,
                &mut typ as *mut _ as *mut std::os::raw::c_void,
            )?;
        }
        self.active = true;
        Ok(())
    }

    fn next(&mut self) -> std::io::Result<Vec<u8>> {
        if !self.active {
            self.start()?;
        }

        unsafe {
            let mut pfd = libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 };
            let ret = libc::poll(&mut pfd, 1, 5_000);
            if ret == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "VIDIOC_DQBUF poll timeout",
                ));
            }
            if ret < 0 {
                return Err(std::io::Error::last_os_error());
            }

            let mut plane: v4l2_plane = std::mem::zeroed();
            let mut buf: v4l2_buffer = std::mem::zeroed();
            buf.type_ = BufType::VideoCaptureMplane as u32;
            buf.memory = v4l::memory::Memory::Mmap as u32;
            buf.length = 1;
            buf.m.planes = &mut plane as *mut _;

            v4l2::ioctl(
                self.fd,
                v4l2::vidioc::VIDIOC_DQBUF,
                &mut buf as *mut _ as *mut std::os::raw::c_void,
            )?;

            let idx = buf.index as usize;
            let (ptr, _len) = self.bufs[idx];
            let used = plane.bytesused as usize;
            let frame = std::slice::from_raw_parts(ptr, used).to_vec();

            let mut plane2: v4l2_plane = std::mem::zeroed();
            let mut qbuf: v4l2_buffer = std::mem::zeroed();
            qbuf.type_ = BufType::VideoCaptureMplane as u32;
            qbuf.memory = v4l::memory::Memory::Mmap as u32;
            qbuf.index = buf.index;
            qbuf.length = 1;
            qbuf.m.planes = &mut plane2 as *mut _;
            v4l2::ioctl(
                self.fd,
                v4l2::vidioc::VIDIOC_QBUF,
                &mut qbuf as *mut _ as *mut std::os::raw::c_void,
            )?;

            Ok(frame)
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for MplaneStream {
    fn drop(&mut self) {
        unsafe {
            if self.active {
                let mut typ = BufType::VideoCaptureMplane as u32;
                let _ = v4l2::ioctl(
                    self.fd,
                    v4l2::vidioc::VIDIOC_STREAMOFF,
                    &mut typ as *mut _ as *mut std::os::raw::c_void,
                );
            }
            for &(ptr, len) in &self.bufs {
                let _ = v4l2::munmap(ptr as *mut std::ffi::c_void, len);
            }
            let mut reqbufs: v4l2_requestbuffers = std::mem::zeroed();
            reqbufs.count = 0;
            reqbufs.type_ = BufType::VideoCaptureMplane as u32;
            reqbufs.memory = v4l::memory::Memory::Mmap as u32;
            let _ = v4l2::ioctl(
                self.fd,
                v4l2::vidioc::VIDIOC_REQBUFS,
                &mut reqbufs as *mut _ as *mut std::os::raw::c_void,
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn v4l2_mplane_enum_formats(dev: &Device) -> std::io::Result<Vec<v4l::format::Description>> {
    let mut formats = Vec::new();
    let mut v4l2_fmt: v4l2_fmtdesc = unsafe { std::mem::zeroed() };
    v4l2_fmt.type_ = BufType::VideoCaptureMplane as u32;

    loop {
        let ret = unsafe {
            v4l2::ioctl(
                dev.handle().fd(),
                v4l2::vidioc::VIDIOC_ENUM_FMT,
                &mut v4l2_fmt as *mut _ as *mut std::os::raw::c_void,
            )
        };
        if ret.is_err() {
            break;
        }
        formats.push(v4l::format::Description::from(v4l2_fmt));
        v4l2_fmt.index += 1;
        unsafe {
            v4l2_fmt.description = std::mem::zeroed();
        }
    }
    Ok(formats)
}

#[cfg(target_os = "linux")]
fn v4l2_mplane_set_format(
    dev: &Device,
    width: u32,
    height: u32,
    fourcc: FourCC,
) -> std::io::Result<MplaneFormat> {
    unsafe {
        let mut v4l2_fmt: v4l2_format = std::mem::zeroed();
        v4l2_fmt.type_ = BufType::VideoCaptureMplane as u32;
        let pix_mp = &mut v4l2_fmt.fmt.pix_mp;
        pix_mp.width = width;
        pix_mp.height = height;
        pix_mp.pixelformat = fourcc.into();
        pix_mp.num_planes = 1;

        v4l2::ioctl(
            dev.handle().fd(),
            v4l2::vidioc::VIDIOC_S_FMT,
            &mut v4l2_fmt as *mut _ as *mut std::os::raw::c_void,
        )?;

        let pix_mp = &v4l2_fmt.fmt.pix_mp;
        Ok(MplaneFormat {
            width: pix_mp.width,
            height: pix_mp.height,
            fourcc: FourCC::from(pix_mp.pixelformat),
            stride: pix_mp.plane_fmt[0].bytesperline,
        })
    }
}

#[cfg(target_os = "linux")]
fn v4l2_mplane_set_fps(dev: &Device, fps: u32) -> std::io::Result<(u32, u32)> {
    let sparm_result = unsafe {
        let mut v4l2_params: v4l2_streamparm = std::mem::zeroed();
        v4l2_params.type_ = BufType::VideoCaptureMplane as u32;
        v4l2_params.parm.capture.timeperframe.numerator = 1;
        v4l2_params.parm.capture.timeperframe.denominator = fps;

        v4l2::ioctl(
            dev.handle().fd(),
            v4l2::vidioc::VIDIOC_S_PARM,
            &mut v4l2_params as *mut _ as *mut std::os::raw::c_void,
        )
        .map(|_| {
            let tf = v4l2_params.parm.capture.timeperframe;
            (tf.denominator, tf.numerator)
        })
    };

    if let Ok(result) = sparm_result {
        return Ok(result);
    }

    info!(
        "VIDIOC_S_PARM not supported; attempting sensor subdevice frame interval for {} fps",
        fps
    );
    set_sensor_subdev_fps(dev, fps)
}

#[cfg(target_os = "linux")]
fn set_sensor_subdev_fps(dev: &Device, fps: u32) -> std::io::Result<(u32, u32)> {
    use std::fs;
    use std::path::Path;

    #[repr(C)]
    struct SubdevFrameInterval {
        pad: u32,
        numerator: u32,
        denominator: u32,
        reserved: [u32; 9],
    }

    const IOC_WRITE: u32 = 1;
    const IOC_READ: u32 = 2;
    fn iowr(ty: u8, nr: u8, size: usize) -> libc::c_ulong {
        (((IOC_READ | IOC_WRITE) as libc::c_ulong) << 30)
            | ((ty as libc::c_ulong) << 8)
            | (nr as libc::c_ulong)
            | ((size as libc::c_ulong) << 16)
    }

    let subdev_s_frame_interval = iowr(b'V', 22, std::mem::size_of::<SubdevFrameInterval>());
    let fd_path = format!("/proc/self/fd/{}", dev.handle().fd());
    let resolved = fs::read_link(&fd_path)?;
    let canonical = fs::canonicalize(&resolved).unwrap_or(resolved);
    let dev_name = canonical.file_name().and_then(|name| name.to_str()).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cannot resolve video device name from fd",
        )
    })?;

    let mut subdev_paths = Vec::new();
    for suffix in ["", "/../"] {
        let dir = format!("/sys/class/video4linux/{}/device{}", dev_name, suffix);
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("v4l-subdev") {
                    subdev_paths.push(format!("/dev/{}", name));
                }
            }
        }
    }

    if subdev_paths.is_empty() {
        for idx in 0..64 {
            let path = format!("/dev/v4l-subdev{}", idx);
            if Path::new(&path).exists() {
                subdev_paths.push(path);
            }
        }
    }
    subdev_paths.sort();
    subdev_paths.dedup();

    for path in subdev_paths {
        let fd = match v4l2::open(&path, libc::O_RDWR | libc::O_NONBLOCK) {
            Ok(fd) => fd,
            Err(_) => continue,
        };

        let mut interval =
            SubdevFrameInterval { pad: 0, numerator: 1, denominator: fps, reserved: [0; 9] };
        let result = unsafe {
            v4l2::ioctl(
                fd,
                subdev_s_frame_interval,
                &mut interval as *mut _ as *mut std::os::raw::c_void,
            )
        };
        unsafe {
            libc::close(fd);
        }
        if result.is_ok() {
            info!("Set sensor frame interval on {}", path);
            return Ok((interval.denominator, interval.numerator));
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("no subdevice accepted VIDIOC_SUBDEV_S_FRAME_INTERVAL for {} fps", fps),
    ))
}

#[cfg(target_os = "linux")]
fn configure_v4l2_capture(args: &Args) -> Result<V4l2CaptureConfig> {
    let device_path = args.device.clone().unwrap_or_else(|| "/dev/video-camera0".to_string());
    let format = V4l2CaptureFormat::from_capture_format(args.format);
    let fourcc = format.fourcc();
    let dev = Device::with_path(&device_path)?;

    let sp_formats = dev.enum_formats().unwrap_or_default();
    let use_mplane = sp_formats.is_empty();

    if use_mplane {
        let mp_formats = v4l2_mplane_enum_formats(&dev)?;
        info!(
            "V4L2 device {} is multiplanar; supports {} format(s)",
            device_path,
            mp_formats.len()
        );
        for format in &mp_formats {
            debug!("V4L2 mplane format: {:?}", format);
        }
        let mf = v4l2_mplane_set_format(&dev, args.width, args.height, fourcc)?;
        match v4l2_mplane_set_fps(&dev, args.fps) {
            Ok((fps_num, fps_den)) => info!("V4L2 framerate set: {}/{}", fps_num, fps_den),
            Err(err) => {
                log::warn!("Could not set V4L2 mplane framerate to {} fps: {}", args.fps, err)
            }
        }
        info!(
            "V4L2 negotiated (mplane): {}x{} fourcc={} stride={}",
            mf.width, mf.height, mf.fourcc, mf.stride
        );
        Ok(V4l2CaptureConfig {
            device_path,
            format,
            width: mf.width,
            height: mf.height,
            stride: mf.stride,
            use_mplane,
        })
    } else {
        info!("V4L2 device {} supports {} format(s)", device_path, sp_formats.len());
        for format in &sp_formats {
            debug!("V4L2 format: {:?}", format);
        }
        let mut fmt = dev.format()?;
        fmt.width = args.width;
        fmt.height = args.height;
        fmt.fourcc = fourcc;
        let fmt = dev.set_format(&fmt)?;
        let params = v4l::video::capture::Parameters::with_fps(args.fps);
        let params = dev.set_params(&params)?;
        info!(
            "V4L2 negotiated: {}x{} fourcc={} stride={}, framerate={}/{}",
            fmt.width,
            fmt.height,
            fmt.fourcc,
            fmt.stride,
            params.interval.denominator,
            params.interval.numerator
        );
        Ok(V4l2CaptureConfig {
            device_path,
            format,
            width: fmt.width,
            height: fmt.height,
            stride: fmt.stride,
            use_mplane,
        })
    }
}

fn list_cameras() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        println!("V4L2 devices:");
        for dev in v4l::context::enum_devices() {
            println!("  {} - {}", dev.path().display(), dev.name().unwrap_or_default());
        }
    }

    let cams = nokhwa::query(ApiBackend::Auto)?;
    println!("Nokhwa cameras:");
    for (i, cam) in cams.iter().enumerate() {
        println!("  {}. {}", i, cam.human_name());
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
    Camera {
        camera: Camera,
        is_yuyv: bool,
    },
    #[cfg(target_os = "linux")]
    V4l2(V4l2CaptureConfig),
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    Argus(argus::ArgusCaptureSession),
}

#[derive(Clone, Copy)]
struct CaptureConfig {
    fps: u32,
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
        SourceKind::V4l2 => {
            #[cfg(target_os = "linux")]
            {
                if args.test_pattern {
                    anyhow::bail!("--test-pattern is not supported with --source v4l2");
                }
                if args.display_video {
                    anyhow::bail!("--display-video is not supported with --source v4l2");
                }
                if args.burn_timestamp {
                    anyhow::bail!("--burn-timestamp is not supported with --source v4l2");
                }
                let capture = configure_v4l2_capture(&args)?;
                (capture.width, capture.height, VideoInput::V4l2(capture))
            }
            #[cfg(not(target_os = "linux"))]
            {
                anyhow::bail!(
                    "--source v4l2 requires Linux; this binary was built for {}-{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                );
            }
        }
        SourceKind::Argus => {
            #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
            {
                if args.test_pattern {
                    anyhow::bail!("--test-pattern is not supported with --source argus");
                }
                if args.display_video {
                    anyhow::bail!("--display-video is not supported with --source argus");
                }
                if args.burn_timestamp {
                    log::warn!(
                        "--burn-timestamp is ignored with --source argus (DMA buffers are not CPU-mapped on the publish path)"
                    );
                }
                let session = argus::ArgusCaptureSession::new(
                    args.camera_index as u32,
                    args.width,
                    args.height,
                    args.fps,
                )?;
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
        SourceKind::Uvc if args.device.is_some() || matches!(args.format, CaptureFormat::Nv12) => {
            #[cfg(target_os = "linux")]
            {
                if args.test_pattern {
                    anyhow::bail!("--test-pattern is not supported with direct V4L2 capture");
                }
                if args.display_video {
                    anyhow::bail!("--display-video is not supported with direct V4L2 capture");
                }
                if args.burn_timestamp {
                    anyhow::bail!("--burn-timestamp is not supported with direct V4L2 capture");
                }
                let capture = configure_v4l2_capture(&args)?;
                (capture.width, capture.height, VideoInput::V4l2(capture))
            }
            #[cfg(not(target_os = "linux"))]
            {
                anyhow::bail!(
                    "--format nv12 and --device require Linux direct V4L2 capture; this binary was built for {}-{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                );
            }
        }
        SourceKind::Uvc => {
            if args.test_pattern {
                let width = args.width;
                let height = args.height;
                let fps = args.fps;
                info!(
                    "Test pattern enabled: SMPTE 75% color bars at {}x{} @ {} fps",
                    width, height, fps
                );
                (width, height, VideoInput::TestPattern(TestPattern::new(width, height)))
            } else {
                // Setup camera
                let index = CameraIndex::Index(args.camera_index as u32);
                let requested = RequestedFormat::new::<RgbFormat>(
                    RequestedFormatType::AbsoluteHighestFrameRate,
                );
                let mut camera = Camera::new(index, requested)?;

                let mut requested_camera_format = None;
                let mut last_request_error = None;
                for frame_format in args.format.frame_formats() {
                    let wanted = CameraFormat::new(
                        Resolution::new(args.width, args.height),
                        *frame_format,
                        args.fps,
                    );
                    match camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
                        RequestedFormatType::Exact(wanted),
                    )) {
                        Ok(format) => {
                            requested_camera_format = Some(format);
                            break;
                        }
                        Err(err) => {
                            last_request_error = Some(err);
                        }
                    }
                }
                if let Some(requested_camera_format) = requested_camera_format {
                    debug!("Requested nokhwa CameraFormat: {:?}", requested_camera_format);
                } else if args.format == CaptureFormat::Auto {
                    if let Some(err) = last_request_error {
                        log::warn!(
                            "Failed to request YUYV or MJPEG at {}x{} @ {} fps; using backend-selected camera format: {}",
                            args.width,
                            args.height,
                            args.fps,
                            err
                        );
                    }
                } else {
                    let formats = args
                        .format
                        .frame_formats()
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" or ");
                    return Err(match last_request_error {
                        Some(err) => anyhow::anyhow!(
                            "failed to request camera format {} at {}x{} @ {} fps: {}",
                            formats,
                            args.width,
                            args.height,
                            args.fps,
                            err
                        ),
                        None => anyhow::anyhow!("no camera capture formats were requested"),
                    });
                }
                camera.open_stream()?;
                let fmt = camera.camera_format();
                let width = fmt.width();
                let height = fmt.height();
                let fps = fmt.frame_rate();
                let is_yuyv = fmt.format() == FrameFormat::YUYV;
                info!(
                    "Camera opened: {}x{} @ {} fps (format: {}, requested: {})",
                    width,
                    height,
                    fps,
                    fmt.format(),
                    args.format
                );
                debug!("Negotiated nokhwa CameraFormat: {:?}", fmt);
                info!(
                    "Selected conversion path: {}",
                    if is_yuyv { "YUYV->I420 (libyuv)" } else { "Auto (RGB24 or MJPEG)" }
                );
                (width, height, VideoInput::Camera { camera, is_yuyv })
            }
        }
    };
    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));
    let display_shared = args.display_video.then(|| Arc::new(Mutex::new(SharedYuv::default())));
    let publish_timing_state =
        args.display_timing.then(|| Arc::new(Mutex::new(PublisherTimingState::default())));

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
    frame_metadata_features.user_timestamp = args.attach_timestamp;
    frame_metadata_features.frame_id = args.attach_frame_id;

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

    let capture_config = CaptureConfig {
        fps: args.fps,
        attach_timestamp: args.attach_timestamp,
        burn_timestamp: args.burn_timestamp,
        attach_frame_id: args.attach_frame_id,
        display_timing: args.display_timing,
    };

    let publish_stats_task =
        tokio::spawn(update_publisher_video_stats(track.clone(), ctrl_c_received.clone()));

    match video_input {
        #[cfg(target_os = "linux")]
        VideoInput::V4l2(capture) => {
            let capture_result = run_v4l2_capture_loop(
                capture_config,
                ctrl_c_received,
                rtc_source,
                capture,
                width,
                height,
            )
            .await;
            let _ = publish_stats_task.await;
            capture_result?;
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        VideoInput::Argus(session) => {
            let capture_result = run_argus_capture_loop(
                capture_config,
                ctrl_c_received,
                rtc_source,
                session,
                width,
                height,
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
                ));

                let display_result = video_display::run_display(
                    "LiveKit Video Publisher",
                    shared,
                    ctrl_c_received.clone(),
                    Some(width as f32 / height as f32),
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
                )
                .await;
                let _ = publish_stats_task.await;
                capture_result?;
            }
        }
    }

    Ok(())
}

async fn run_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    track: LocalVideoTrack,
    rtc_source: NativeVideoSource,
    mut video_input: VideoInput,
    width: u32,
    height: u32,
    display_shared: Option<Arc<Mutex<SharedYuv>>>,
    publish_timing_state: Option<Arc<Mutex<PublisherTimingState>>>,
) -> Result<()> {
    // Pace publishing at the requested FPS (not the camera-reported FPS) to hit desired cadence
    let pace_fps = config.fps as f64;
    // Accurate pacing using absolute schedule (no drift)
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / pace_fps));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Align the first tick to now
    ticker.tick().await;
    let start_ts = Instant::now();

    // Capture loop
    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let mut fps_window_frames: u64 = 0;
    let mut fps_window_start = Instant::now();
    let mut fps_smoothed: f32 = 0.0;
    let target = Duration::from_secs_f64(1.0 / pace_fps);
    info!("Target frame interval: {:.2} ms", target.as_secs_f64() * 1000.0);

    // Timing accumulators (ms) for rolling stats
    let mut timings = PublisherTimingSummary::default();
    let mut logged_mjpeg_fallback = false;
    let mut capture_timestamp_log_state = CaptureTimestampLogState::default();
    let mut frame_counter: u32 = 1;
    let mut timestamp_overlay = (config.attach_timestamp && config.burn_timestamp)
        .then(|| TimestampOverlay::new(width, height));
    let align_buffers_for_display = display_shared.is_some();

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        // Wait until the scheduled next frame time
        let paced_wait_started_at = Instant::now();
        ticker.tick().await;
        let paced_wait_finished_at = Instant::now();

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
        let stride_y_usize = stride_y as usize;

        let source_frame_started_at = Instant::now();
        let frame_wall_time_us = unix_time_us_now();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        let (
            capture_wall_time_us,
            read_wall_time_us,
            source_frame_acquired_at,
            decode_finished_at,
            convert_finished_at,
            used_decode_path,
            record_convert_timing,
        ) = match &mut video_input {
            VideoInput::TestPattern(pattern) => {
                pattern.render(
                    data_y,
                    stride_y as i32,
                    data_u,
                    stride_u as i32,
                    data_v,
                    stride_v as i32,
                );
                let frame_acquired_at = Instant::now();
                (
                    frame_wall_time_us,
                    unix_time_us_now(),
                    frame_acquired_at,
                    frame_acquired_at,
                    frame_acquired_at,
                    false,
                    false,
                )
            }
            VideoInput::Camera { camera, is_yuyv } => {
                // Capture the frame as early as possible so the attached timestamp is
                // close to the camera acquisition point.
                let frame_buf = camera.frame()?;
                let read_wall_time_us = unix_time_us_now();
                let camera_frame_acquired_at = Instant::now();

                // Prefer backend capture timestamps only when they are plausible Unix
                // wall-clock times. Some camera APIs expose stream-relative or future
                // presentation timestamps; attaching those makes latency appear negative.
                let capture_wall_time_us = select_capture_wall_time_us(
                    frame_buf.capture_timestamp(),
                    frame_wall_time_us,
                    read_wall_time_us,
                    &mut capture_timestamp_log_state,
                );

                let (decode_finished_at, convert_finished_at, used_decode_path) = if *is_yuyv {
                    // Fast path for YUYV: convert directly to I420 via libyuv
                    let src = frame_buf.buffer();
                    let src_bytes = src.as_ref();
                    let src_stride = (width * 2) as i32; // YUYV packed 4:2:2
                    unsafe {
                        // returns 0 on success
                        let _ = yuv_sys::rs_YUY2ToI420(
                            src_bytes.as_ptr(),
                            src_stride,
                            data_y.as_mut_ptr(),
                            stride_y as i32,
                            data_u.as_mut_ptr(),
                            stride_u as i32,
                            data_v.as_mut_ptr(),
                            stride_v as i32,
                            width as i32,
                            height as i32,
                        );
                    }
                    (camera_frame_acquired_at, Instant::now(), false)
                } else {
                    // Auto path (either RGB24 already or compressed MJPEG)
                    let src = frame_buf.buffer();
                    if src.len() == (width as usize * height as usize * 3) {
                        // Already RGB24 from backend; convert directly
                        unsafe {
                            let _ = yuv_sys::rs_RGB24ToI420(
                                src.as_ref().as_ptr(),
                                (width * 3) as i32,
                                data_y.as_mut_ptr(),
                                stride_y as i32,
                                data_u.as_mut_ptr(),
                                stride_u as i32,
                                data_v.as_mut_ptr(),
                                stride_v as i32,
                                width as i32,
                                height as i32,
                            );
                        }
                        (camera_frame_acquired_at, Instant::now(), false)
                    } else {
                        // Try fast MJPEG->I420 via libyuv if available; fallback to image crate
                        let mut used_fast_mjpeg = false;
                        let fast_mjpeg_buffer_ready_at = unsafe {
                            // rs_MJPGToI420 returns 0 on success
                            let ret = yuv_sys::rs_MJPGToI420(
                                src.as_ref().as_ptr(),
                                src.len(),
                                data_y.as_mut_ptr(),
                                stride_y as i32,
                                data_u.as_mut_ptr(),
                                stride_u as i32,
                                data_v.as_mut_ptr(),
                                stride_v as i32,
                                width as i32,
                                height as i32,
                                width as i32,
                                height as i32,
                            );
                            if ret == 0 {
                                used_fast_mjpeg = true;
                                Instant::now()
                            } else {
                                camera_frame_acquired_at
                            }
                        };
                        if used_fast_mjpeg {
                            (fast_mjpeg_buffer_ready_at, fast_mjpeg_buffer_ready_at, true)
                        } else {
                            // Fallback: decode MJPEG using image crate then RGB24->I420
                            match image::load_from_memory(src.as_ref()) {
                                Ok(img_dyn) => {
                                    let rgb8 = img_dyn.to_rgb8();
                                    let decode_finished_at = Instant::now();
                                    let dec_w = rgb8.width() as u32;
                                    let dec_h = rgb8.height() as u32;
                                    if dec_w != width || dec_h != height {
                                        log::warn!(
                                            "Decoded MJPEG size {}x{} differs from requested {}x{}; dropping frame",
                                            dec_w, dec_h, width, height
                                        );
                                        continue;
                                    }
                                    unsafe {
                                        let _ = yuv_sys::rs_RGB24ToI420(
                                            rgb8.as_raw().as_ptr(),
                                            (dec_w * 3) as i32,
                                            data_y.as_mut_ptr(),
                                            stride_y as i32,
                                            data_u.as_mut_ptr(),
                                            stride_u as i32,
                                            data_v.as_mut_ptr(),
                                            stride_v as i32,
                                            width as i32,
                                            height as i32,
                                        );
                                    }
                                    (decode_finished_at, Instant::now(), true)
                                }
                                Err(e2) => {
                                    if !logged_mjpeg_fallback {
                                        log::error!(
                                            "MJPEG decode failed; buffer not RGB24 and image decode failed: {}",
                                            e2
                                        );
                                        logged_mjpeg_fallback = true;
                                    }
                                    continue;
                                }
                            }
                        }
                    }
                };

                (
                    capture_wall_time_us,
                    read_wall_time_us,
                    camera_frame_acquired_at,
                    decode_finished_at,
                    convert_finished_at,
                    used_decode_path,
                    true,
                )
            }
            #[cfg(target_os = "linux")]
            VideoInput::V4l2(_) => {
                unreachable!("direct V4L2 capture must be driven by run_v4l2_capture_loop")
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
            timing_state.lock().record_frame_buffer(capture_wall_time_us, read_wall_time_us, fid);
        }
        let mut buffer_ready_at = convert_finished_at;
        let mut frame_draw_ms = None;
        let mut burned_timestamp_us = None;
        if let Some(overlay) = timestamp_overlay.as_mut() {
            let overlay_started_at = Instant::now();
            overlay.draw(data_y, stride_y_usize, capture_wall_time_us, fid);
            burned_timestamp_us = Some(capture_wall_time_us);
            let overlay_finished_at = Instant::now();
            frame_draw_ms = Some((overlay_finished_at - overlay_started_at).as_secs_f64() * 1000.0);
            buffer_ready_at = overlay_finished_at;
        }

        // Build frame metadata from enabled packet trailer features and local timing correlation.
        let user_ts = if config.attach_timestamp || config.display_timing {
            Some(capture_wall_time_us)
        } else {
            None
        };
        if burned_timestamp_us.is_some() {
            debug_assert_eq!(burned_timestamp_us, Some(capture_wall_time_us));
        }
        frame.frame_metadata = if user_ts.is_some() || fid.is_some() {
            Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
        } else {
            None
        };
        // Monotonic, microseconds since start.
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        rtc_source.capture_frame(&frame);
        let webrtc_capture_finished_at = Instant::now();
        if let Some(shared) = display_shared.as_ref() {
            let (stride_y, stride_u, stride_v) = frame.buffer.strides();
            let (data_y, data_u, data_v) = frame.buffer.data();
            let timing_sample = if config.display_timing {
                publish_timing_state
                    .as_ref()
                    .and_then(|timing_state| timing_state.lock().display_sample())
            } else {
                None
            };
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
            .record((source_frame_acquired_at - source_frame_started_at).as_secs_f64() * 1000.0);
        if used_decode_path {
            timings
                .decode_mjpeg_ms
                .record((decode_finished_at - source_frame_acquired_at).as_secs_f64() * 1000.0);
        }
        if record_convert_timing {
            timings
                .buffer_convert_ms
                .record((convert_finished_at - decode_finished_at).as_secs_f64() * 1000.0);
        }
        if let Some(frame_draw_ms) = frame_draw_ms {
            timings.frame_draw_ms.record(frame_draw_ms);
        }
        timings
            .submit_to_webrtc_ms
            .record((webrtc_capture_finished_at - buffer_ready_at).as_secs_f64() * 1000.0);
        timings
            .capture_to_webrtc_total_ms
            .record((webrtc_capture_finished_at - source_frame_started_at).as_secs_f64() * 1000.0);

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

#[cfg(target_os = "linux")]
async fn run_v4l2_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    capture: V4l2CaptureConfig,
    width: u32,
    height: u32,
) -> Result<()> {
    let dev = Device::with_path(&capture.device_path)?;
    let target = Duration::from_secs_f64(1.0 / config.fps as f64);
    let start_ts = Instant::now();

    if capture.use_mplane {
        let negotiated =
            v4l2_mplane_set_format(&dev, capture.width, capture.height, capture.format.fourcc())?;
        if negotiated.width != width || negotiated.height != height {
            anyhow::bail!(
                "V4L2 mplane renegotiated {}x{} after publishing {}x{}",
                negotiated.width,
                negotiated.height,
                width,
                height
            );
        }
        if let Err(err) = v4l2_mplane_set_fps(&dev, config.fps) {
            log::warn!("Could not set V4L2 mplane framerate to {} fps: {}", config.fps, err);
        }
        let mut stream = MplaneStream::new(&dev, 4)?;
        if capture.format == V4l2CaptureFormat::Nv12 {
            info!("Direct V4L2 NV12 mplane capture started: camera -> NV12Buffer -> encoder");
            run_v4l2_nv12_loop_mplane(
                &rtc_source,
                &mut stream,
                negotiated.stride,
                width,
                height,
                target,
                start_ts,
                &ctrl_c_received,
                config,
            )?;
        } else {
            info!("Direct V4L2 mplane capture started: camera -> I420Buffer -> encoder");
            run_v4l2_convert_loop_mplane(
                &rtc_source,
                &mut stream,
                capture.format,
                width,
                height,
                target,
                start_ts,
                &ctrl_c_received,
                config,
            )?;
        }
    } else {
        let mut fmt = dev.format()?;
        fmt.width = capture.width;
        fmt.height = capture.height;
        fmt.fourcc = capture.format.fourcc();
        let fmt = dev.set_format(&fmt)?;
        if fmt.width != width || fmt.height != height {
            anyhow::bail!(
                "V4L2 renegotiated {}x{} after publishing {}x{}",
                fmt.width,
                fmt.height,
                width,
                height
            );
        }
        let params = v4l::video::capture::Parameters::with_fps(config.fps);
        if let Err(err) = dev.set_params(&params) {
            log::warn!("Could not set V4L2 framerate to {} fps: {}", config.fps, err);
        }
        let mut stream = v4l::io::mmap::Stream::with_buffers(&dev, BufType::VideoCapture, 4)?;
        if capture.format == V4l2CaptureFormat::Nv12 {
            info!("Direct V4L2 NV12 capture started: camera -> NV12Buffer -> encoder");
            run_v4l2_nv12_loop(
                &rtc_source,
                &mut stream,
                fmt.stride,
                width,
                height,
                target,
                start_ts,
                &ctrl_c_received,
                config,
            )?;
        } else {
            info!("Direct V4L2 capture started: camera -> I420Buffer -> encoder");
            run_v4l2_convert_loop(
                &rtc_source,
                &mut stream,
                capture.format,
                width,
                height,
                target,
                start_ts,
                &ctrl_c_received,
                config,
            )?;
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn next_frame_metadata(
    config: CaptureConfig,
    capture_wall_time_us: u64,
    frame_counter: &mut u32,
) -> Option<FrameMetadata> {
    let user_ts = config.attach_timestamp.then_some(capture_wall_time_us);
    let fid = if config.attach_frame_id {
        let id = *frame_counter;
        *frame_counter = (*frame_counter).wrapping_add(1);
        Some(id)
    } else {
        None
    };

    if user_ts.is_some() || fid.is_some() {
        Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn copy_nv12_to_buffer(src: &[u8], dst: &mut NV12Buffer, stride: u32, height: u32) {
    let src_stride_y = stride as usize;
    let src_stride_uv = stride as usize;
    let y_plane_size = src_stride_y * height as usize;
    let uv_plane_size = src_stride_uv * ((height as usize + 1) / 2);
    let (dst_y, dst_uv) = dst.data_mut();
    let copy_y = y_plane_size.min(dst_y.len()).min(src.len());
    dst_y[..copy_y].copy_from_slice(&src[..copy_y]);
    let uv_start = y_plane_size;
    let copy_uv = uv_plane_size.min(dst_uv.len()).min(src.len().saturating_sub(uv_start));
    dst_uv[..copy_uv].copy_from_slice(&src[uv_start..uv_start + copy_uv]);
}

#[cfg(target_os = "linux")]
fn run_v4l2_nv12_loop(
    rtc_source: &NativeVideoSource,
    stream: &mut v4l::io::mmap::Stream,
    stride: u32,
    width: u32,
    height: u32,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
    config: CaptureConfig,
) -> Result<()> {
    let mut nv12_buf = NV12Buffer::with_strides(width, height, stride, stride);
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: NV12Buffer::new(width, height),
    };

    let mut frames = 0_u64;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_copy_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors = 0_u32;
    let mut frame_counter = 1_u32;
    const MAX_ERRORS: u32 = 30;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let t0 = Instant::now();
        let (buf, _meta) = match stream.next() {
            Ok(item) => {
                consecutive_errors = 0;
                item
            }
            Err(err) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    anyhow::bail!(
                        "V4L2 capture failed {} consecutive times: {}",
                        consecutive_errors,
                        err
                    );
                }
                log::warn!("V4L2 error ({}/{}): {}", consecutive_errors, MAX_ERRORS, err);
                continue;
            }
        };
        let read_wall_time_us = unix_time_us_now();
        let t1 = Instant::now();

        copy_nv12_to_buffer(buf, &mut nv12_buf, stride, height);
        let t2 = Instant::now();

        std::mem::swap(&mut frame.buffer, &mut nv12_buf);
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        frame.frame_metadata = next_frame_metadata(config, read_wall_time_us, &mut frame_counter);
        rtc_source.capture_frame(&frame);
        std::mem::swap(&mut frame.buffer, &mut nv12_buf);
        let t3 = Instant::now();

        frames += 1;
        sum_get_ms += (t1 - t0).as_secs_f64() * 1000.0;
        sum_copy_ms += (t2 - t1).as_secs_f64() * 1000.0;
        sum_capture_ms += (t3 - t2).as_secs_f64() * 1000.0;

        if last_fps_log.elapsed() >= Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let n = frames.max(1) as f64;
            info!(
                "V4L2 NV12 publishing: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, copy {:.2}, submit {:.2} | target {:.2}",
                width,
                height,
                fps_est,
                sum_get_ms / n,
                sum_copy_ms / n,
                sum_capture_ms / n,
                target.as_secs_f64() * 1000.0,
            );
            frames = 0;
            sum_get_ms = 0.0;
            sum_copy_ms = 0.0;
            sum_capture_ms = 0.0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn run_v4l2_nv12_loop_mplane(
    rtc_source: &NativeVideoSource,
    stream: &mut MplaneStream,
    stride: u32,
    width: u32,
    height: u32,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
    config: CaptureConfig,
) -> Result<()> {
    let mut nv12_buf = NV12Buffer::with_strides(width, height, stride, stride);
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: NV12Buffer::new(width, height),
    };

    let mut frames = 0_u64;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_copy_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors = 0_u32;
    let mut frame_counter = 1_u32;
    const MAX_ERRORS: u32 = 30;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let t0 = Instant::now();
        let buf = match stream.next() {
            Ok(buf) => {
                consecutive_errors = 0;
                buf
            }
            Err(err) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    anyhow::bail!(
                        "V4L2 mplane capture failed {} consecutive times: {}",
                        consecutive_errors,
                        err
                    );
                }
                log::warn!("V4L2 mplane error ({}/{}): {}", consecutive_errors, MAX_ERRORS, err);
                continue;
            }
        };
        let read_wall_time_us = unix_time_us_now();
        let t1 = Instant::now();

        copy_nv12_to_buffer(&buf, &mut nv12_buf, stride, height);
        let t2 = Instant::now();

        std::mem::swap(&mut frame.buffer, &mut nv12_buf);
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        frame.frame_metadata = next_frame_metadata(config, read_wall_time_us, &mut frame_counter);
        rtc_source.capture_frame(&frame);
        std::mem::swap(&mut frame.buffer, &mut nv12_buf);
        let t3 = Instant::now();

        frames += 1;
        sum_get_ms += (t1 - t0).as_secs_f64() * 1000.0;
        sum_copy_ms += (t2 - t1).as_secs_f64() * 1000.0;
        sum_capture_ms += (t3 - t2).as_secs_f64() * 1000.0;

        if last_fps_log.elapsed() >= Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let n = frames.max(1) as f64;
            info!(
                "V4L2 NV12 mplane publishing: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, copy {:.2}, submit {:.2} | target {:.2}",
                width,
                height,
                fps_est,
                sum_get_ms / n,
                sum_copy_ms / n,
                sum_capture_ms / n,
                target.as_secs_f64() * 1000.0,
            );
            frames = 0;
            sum_get_ms = 0.0;
            sum_copy_ms = 0.0;
            sum_capture_ms = 0.0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn convert_v4l2_frame_to_i420(
    format: V4l2CaptureFormat,
    src: &[u8],
    width: u32,
    height: u32,
    data_y: &mut [u8],
    stride_y: i32,
    data_u: &mut [u8],
    stride_u: i32,
    data_v: &mut [u8],
    stride_v: i32,
) -> bool {
    match format {
        V4l2CaptureFormat::Nv12 => unreachable!("NV12 is handled by the NV12 capture loop"),
        V4l2CaptureFormat::Yuyv => {
            unsafe {
                yuv_sys::rs_YUY2ToI420(
                    src.as_ptr(),
                    (width * 2) as i32,
                    data_y.as_mut_ptr(),
                    stride_y,
                    data_u.as_mut_ptr(),
                    stride_u,
                    data_v.as_mut_ptr(),
                    stride_v,
                    width as i32,
                    height as i32,
                );
            }
            true
        }
        V4l2CaptureFormat::Mjpeg => {
            let ret = unsafe {
                yuv_sys::rs_MJPGToI420(
                    src.as_ptr(),
                    src.len(),
                    data_y.as_mut_ptr(),
                    stride_y,
                    data_u.as_mut_ptr(),
                    stride_u,
                    data_v.as_mut_ptr(),
                    stride_v,
                    width as i32,
                    height as i32,
                    width as i32,
                    height as i32,
                )
            };
            if ret != 0 {
                log::warn!("MJPGToI420 failed (ret={}); skipping frame", ret);
                return false;
            }
            true
        }
    }
}

#[cfg(target_os = "linux")]
fn run_v4l2_convert_loop(
    rtc_source: &NativeVideoSource,
    stream: &mut v4l::io::mmap::Stream,
    format: V4l2CaptureFormat,
    width: u32,
    height: u32,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
    config: CaptureConfig,
) -> Result<()> {
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: I420Buffer::new(width, height),
    };

    let mut frames = 0_u64;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_convert_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors = 0_u32;
    let mut frame_counter = 1_u32;
    const MAX_ERRORS: u32 = 30;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let t0 = Instant::now();
        let (buf, _meta) = match stream.next() {
            Ok(item) => {
                consecutive_errors = 0;
                item
            }
            Err(err) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    anyhow::bail!(
                        "V4L2 capture failed {} consecutive times: {}",
                        consecutive_errors,
                        err
                    );
                }
                log::warn!("V4L2 error ({}/{}): {}", consecutive_errors, MAX_ERRORS, err);
                continue;
            }
        };
        let read_wall_time_us = unix_time_us_now();
        let t1 = Instant::now();

        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        if !convert_v4l2_frame_to_i420(
            format,
            buf,
            width,
            height,
            data_y,
            stride_y as i32,
            data_u,
            stride_u as i32,
            data_v,
            stride_v as i32,
        ) {
            continue;
        }
        let t2 = Instant::now();

        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        frame.frame_metadata = next_frame_metadata(config, read_wall_time_us, &mut frame_counter);
        rtc_source.capture_frame(&frame);
        let t3 = Instant::now();

        frames += 1;
        sum_get_ms += (t1 - t0).as_secs_f64() * 1000.0;
        sum_convert_ms += (t2 - t1).as_secs_f64() * 1000.0;
        sum_capture_ms += (t3 - t2).as_secs_f64() * 1000.0;

        if last_fps_log.elapsed() >= Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let n = frames.max(1) as f64;
            info!(
                "V4L2 {:?} publishing: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, convert {:.2}, submit {:.2} | target {:.2}",
                format,
                width,
                height,
                fps_est,
                sum_get_ms / n,
                sum_convert_ms / n,
                sum_capture_ms / n,
                target.as_secs_f64() * 1000.0,
            );
            frames = 0;
            sum_get_ms = 0.0;
            sum_convert_ms = 0.0;
            sum_capture_ms = 0.0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn run_v4l2_convert_loop_mplane(
    rtc_source: &NativeVideoSource,
    stream: &mut MplaneStream,
    format: V4l2CaptureFormat,
    width: u32,
    height: u32,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
    config: CaptureConfig,
) -> Result<()> {
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: I420Buffer::new(width, height),
    };

    let mut frames = 0_u64;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_convert_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors = 0_u32;
    let mut frame_counter = 1_u32;
    const MAX_ERRORS: u32 = 30;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let t0 = Instant::now();
        let buf = match stream.next() {
            Ok(buf) => {
                consecutive_errors = 0;
                buf
            }
            Err(err) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    anyhow::bail!(
                        "V4L2 mplane capture failed {} consecutive times: {}",
                        consecutive_errors,
                        err
                    );
                }
                log::warn!("V4L2 mplane error ({}/{}): {}", consecutive_errors, MAX_ERRORS, err);
                continue;
            }
        };
        let read_wall_time_us = unix_time_us_now();
        let t1 = Instant::now();

        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        if !convert_v4l2_frame_to_i420(
            format,
            &buf,
            width,
            height,
            data_y,
            stride_y as i32,
            data_u,
            stride_u as i32,
            data_v,
            stride_v as i32,
        ) {
            continue;
        }
        let t2 = Instant::now();

        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        frame.frame_metadata = next_frame_metadata(config, read_wall_time_us, &mut frame_counter);
        rtc_source.capture_frame(&frame);
        let t3 = Instant::now();

        frames += 1;
        sum_get_ms += (t1 - t0).as_secs_f64() * 1000.0;
        sum_convert_ms += (t2 - t1).as_secs_f64() * 1000.0;
        sum_capture_ms += (t3 - t2).as_secs_f64() * 1000.0;

        if last_fps_log.elapsed() >= Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let n = frames.max(1) as f64;
            info!(
                "V4L2 {:?} mplane publishing: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, convert {:.2}, submit {:.2} | target {:.2}",
                format,
                width,
                height,
                fps_est,
                sum_get_ms / n,
                sum_convert_ms / n,
                sum_capture_ms / n,
                target.as_secs_f64() * 1000.0,
            );
            frames = 0;
            sum_get_ms = 0.0;
            sum_convert_ms = 0.0;
            sum_capture_ms = 0.0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

/// Capture loop dedicated to Jetson MIPI capture via libargus.
///
/// Argus blocks inside `acquireFrame`, pacing capture itself, so this loop runs in a
/// dedicated OS thread and pushes NV12 DMA-buffer fds straight into `NativeVideoSource`
/// via [`NativeVideoSource::capture_dmabuf_frame_with_metadata`] for zero-copy hand-off
/// to the Jetson hardware encoder.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
async fn run_argus_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    session: argus::ArgusCaptureSession,
    width: u32,
    height: u32,
) -> Result<()> {
    let capture_handle = std::thread::Builder::new()
        .name("mipi-capture".into())
        .spawn(move || -> Result<()> {
            let mut session = session;
            let start_ts = Instant::now();
            let mut frames: u64 = 0;
            let mut last_fps_log = Instant::now();
            let mut sum_acquire_ms = 0.0;
            let mut sum_argus_wait_ms = 0.0;
            let mut sum_argus_blit_ms = 0.0;
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

            loop {
                if ctrl_c_received.load(Ordering::Acquire) {
                    break;
                }

                let iter_start = Instant::now();
                let acquire_started_at = Instant::now();
                let argus_frame = match session.acquire_frame() {
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
                        let sensor_to_acquire_ms = fallback_wall_time_us
                            .saturating_sub(capture_wall_time_us)
                            as f64
                            / 1_000.0;
                        let blit_ms = argus_frame.blit_ns as f64 / 1_000_000.0;
                        sum_sensor_to_acquire_ms += sensor_to_acquire_ms;
                        sum_sensor_to_argus_acquire_ms +=
                            (sensor_to_acquire_ms - blit_ms).max(0.0);
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
                let frame_metadata = if user_ts.is_some() || fid.is_some() {
                    Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
                } else {
                    None
                };

                rtc_source.capture_dmabuf_frame_with_metadata(
                    argus_frame.dmabuf_fd,
                    width,
                    height,
                    0, // NV12
                    start_ts.elapsed().as_micros() as i64,
                    frame_metadata,
                );
                let capture_finished_at = Instant::now();

                frames += 1;
                sum_acquire_ms += (acquire_finished_at - acquire_started_at).as_secs_f64() * 1000.0;
                sum_argus_wait_ms += argus_frame.acquire_wait_ns as f64 / 1_000_000.0;
                sum_argus_blit_ms += argus_frame.blit_ns as f64 / 1_000_000.0;
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
