use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{
    self, video as video_presets, PacketTrailerFeatures, TrackPublishOptions, VideoCodec,
    VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
use livekit::webrtc::desktop_capturer::{
    CaptureError, CaptureSource, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
    DesktopFrame,
};
use livekit::webrtc::native::yuv_helper;
#[cfg(target_os = "macos")]
use livekit::webrtc::video_frame::native::{NativeBuffer, VideoFrameBufferExt};
use livekit::webrtc::video_frame::{
    FrameMetadata, I420Buffer, VideoBuffer, VideoFrame, VideoRotation,
};
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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, env};
use yuv_sys;

mod test_pattern;
mod timestamp_burn;
mod video_display;
mod viewport_aspect;

use test_pattern::TestPattern;
use timestamp_burn::{LatencyDisplay, TimestampOverlay};
use video_display::{align_up, PublisherTimingSample, SharedYuv};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// Camera index to use (numeric)
    #[arg(long, conflicts_with_all = ["screen_index", "test_pattern"])]
    camera_index: Option<usize>,

    /// Screen index to capture instead of a camera (zero-based)
    #[arg(long, conflicts_with_all = ["camera_index", "test_pattern"])]
    screen_index: Option<usize>,

    /// Generate a standard SMPTE color-bar test pattern instead of using a camera
    #[arg(long, default_value_t = false, conflicts_with = "list_cameras")]
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

    /// Use H.265/HEVC encoding if supported (falls back to H.264 on failure)
    #[arg(long, default_value_t = false)]
    h265: bool,

    /// Attach the current system time (microseconds since UNIX epoch) as the user timestamp on each frame
    #[arg(long, default_value_t = false)]
    attach_timestamp: bool,

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

fn is_twirp_not_found(err: &ServiceError) -> bool {
    matches!(
        err,
        ServiceError::Twirp(TwirpError::Twirp(code))
            if code.code == TwirpErrorCode::NOT_FOUND
    )
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

/// Format the us delta as a millisecond string like `"12.3ms"`.
fn format_us_delta_ms(later_us: u64, earlier_us: u64) -> String {
    let delta_us = later_us.saturating_sub(earlier_us);
    format!("{:.1}ms", delta_us as f64 / 1_000.0)
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

#[derive(Clone, Copy, Debug)]
struct OutboundVideoSnapshot {
    frames_encoded: u32,
    frames_sent: u32,
    packets_sent: u64,
    bytes_sent: u64,
    key_frames_encoded: u32,
    total_encode_time: f64,
    total_packet_send_delay: f64,
    sampled_at: Instant,
}

impl OutboundVideoSnapshot {
    fn from_stats(outbound: &livekit::webrtc::stats::OutboundRtpStats) -> Self {
        Self {
            frames_encoded: outbound.outbound.frames_encoded,
            frames_sent: outbound.outbound.frames_sent,
            packets_sent: outbound.sent.packets_sent,
            bytes_sent: outbound.sent.bytes_sent,
            key_frames_encoded: outbound.outbound.key_frames_encoded,
            total_encode_time: outbound.outbound.total_encode_time,
            total_packet_send_delay: outbound.outbound.total_packet_send_delay,
            sampled_at: Instant::now(),
        }
    }
}

fn outbound_video_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
) -> Vec<livekit::webrtc::stats::OutboundRtpStats> {
    stats
        .iter()
        .filter_map(|stat| match stat {
            livekit::webrtc::stats::RtcStats::OutboundRtp(outbound)
                if outbound.stream.kind == "video" =>
            {
                Some(outbound.clone())
            }
            _ => None,
        })
        .collect()
}

fn outbound_label(outbound: &livekit::webrtc::stats::OutboundRtpStats) -> String {
    if outbound.outbound.rid.is_empty() {
        outbound.rtc.id.clone()
    } else {
        outbound.outbound.rid.clone()
    }
}

fn log_publisher_outbound_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
    previous: &mut HashMap<String, OutboundVideoSnapshot>,
) {
    let outbounds = outbound_video_stats(stats);
    if outbounds.is_empty() {
        debug!("Publisher outbound stats: no video outbound RTP stats yet");
        return;
    }

    for outbound in outbounds {
        let label = outbound_label(&outbound);
        let current = OutboundVideoSnapshot::from_stats(&outbound);
        let elapsed = previous
            .get(&label)
            .map(|prev| current.sampled_at.saturating_duration_since(prev.sampled_at))
            .unwrap_or_default()
            .as_secs_f64()
            .max(0.001);

        let (
            encoded_delta,
            sent_delta,
            packets_delta,
            bytes_delta,
            keyframe_delta,
            encode_time_delta,
            packet_send_delay_delta,
        ) = previous.get(&label).map_or((0, 0, 0, 0, 0, 0.0, 0.0), |prev| {
            (
                current.frames_encoded.saturating_sub(prev.frames_encoded),
                current.frames_sent.saturating_sub(prev.frames_sent),
                current.packets_sent.saturating_sub(prev.packets_sent),
                current.bytes_sent.saturating_sub(prev.bytes_sent),
                current.key_frames_encoded.saturating_sub(prev.key_frames_encoded),
                (current.total_encode_time - prev.total_encode_time).max(0.0),
                (current.total_packet_send_delay - prev.total_packet_send_delay).max(0.0),
            )
        });

        let bitrate_kbps = bytes_delta as f64 * 8.0 / elapsed / 1000.0;
        let encode_ms_per_frame =
            if encoded_delta > 0 { encode_time_delta * 1000.0 / encoded_delta as f64 } else { 0.0 };
        let send_delay_ms_per_packet = if packets_delta > 0 {
            packet_send_delay_delta * 1000.0 / packets_delta as f64
        } else {
            0.0
        };

        if previous.contains_key(&label) && outbound.outbound.active && sent_delta == 0 {
            log::warn!(
                "Publisher outbound stalled: layer={} active={} encoded+{} sent+{} packets+{} bytes+{} target={:.0}bps quality={:?}",
                label,
                outbound.outbound.active,
                encoded_delta,
                sent_delta,
                packets_delta,
                bytes_delta,
                outbound.outbound.target_bitrate,
                outbound.outbound.quality_limitation_reason,
            );
        } else {
            info!(
                "Publisher outbound: layer={} active={} {}x{} fps={:.1} encoded+{} sent+{} keyframes+{} bitrate={:.0}kbps target={:.0}bps encode={:.2}ms/frame send_delay={:.2}ms/packet quality={:?} encoder={}",
                label,
                outbound.outbound.active,
                outbound.outbound.frame_width,
                outbound.outbound.frame_height,
                outbound.outbound.frames_per_second,
                encoded_delta,
                sent_delta,
                keyframe_delta,
                bitrate_kbps,
                outbound.outbound.target_bitrate,
                encode_ms_per_frame,
                send_delay_ms_per_packet,
                outbound.outbound.quality_limitation_reason,
                outbound.outbound.encoder_implementation,
            );
        }

        previous.insert(label, current);
    }
}

fn spawn_publisher_outbound_stats_logger(track: LocalVideoTrack, ctrl_c_received: Arc<AtomicBool>) {
    tokio::spawn(async move {
        let mut previous = HashMap::new();
        let mut logged_error = false;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if ctrl_c_received.load(Ordering::Acquire) {
                break;
            }

            match track.get_stats().await {
                Ok(stats) => {
                    logged_error = false;
                    log_publisher_outbound_stats(&stats, &mut previous);
                }
                Err(err) if !logged_error => {
                    debug!("Failed to get publisher outbound stats: {:?}", err);
                    logged_error = true;
                }
                Err(_) => {}
            }

            interval.tick().await;
        }
    });
}

fn list_cameras() -> Result<()> {
    let cams = nokhwa::query(ApiBackend::Auto)?;
    println!("Available cameras:");
    for (i, cam) in cams.iter().enumerate() {
        println!("{}. {}", i, cam.human_name());
    }
    Ok(())
}

enum VideoInput {
    TestPattern(TestPattern),
    Camera {
        camera: Camera,
        is_yuyv: bool,
    },
    Screen(ScreenInput),
    #[cfg(target_os = "macos")]
    MacScreen(MacScreenInput),
}

enum PublishBuffer {
    I420(I420Buffer),
    #[cfg(target_os = "macos")]
    Native(NativeBuffer),
}

impl PublishBuffer {
    fn i420_mut(&mut self) -> Option<&mut I420Buffer> {
        match self {
            Self::I420(buffer) => Some(buffer),
            #[cfg(target_os = "macos")]
            Self::Native(_) => None,
        }
    }

    fn i420_for_display(&self) -> Option<I420DisplayBuffer<'_>> {
        match self {
            Self::I420(buffer) => Some(I420DisplayBuffer::Borrowed(buffer)),
            #[cfg(target_os = "macos")]
            Self::Native(buffer) => Some(I420DisplayBuffer::Owned(buffer.to_i420())),
        }
    }
}

impl AsRef<dyn VideoBuffer> for PublishBuffer {
    fn as_ref(&self) -> &(dyn VideoBuffer + 'static) {
        match self {
            Self::I420(buffer) => buffer.as_ref(),
            #[cfg(target_os = "macos")]
            Self::Native(buffer) => buffer.as_ref(),
        }
    }
}

enum I420DisplayBuffer<'a> {
    Borrowed(&'a I420Buffer),
    Owned(I420Buffer),
}

impl I420DisplayBuffer<'_> {
    fn buffer(&self) -> &I420Buffer {
        match self {
            Self::Borrowed(buffer) => buffer,
            Self::Owned(buffer) => buffer,
        }
    }
}

struct ScreenFrame {
    width: u32,
    height: u32,
    capture_wall_time_us: u64,
    buffer: I420Buffer,
}

struct ScreenInput {
    capturer: DesktopCapturer,
    frame_rx: mpsc::Receiver<ScreenFrame>,
    pending_frame: Option<ScreenFrame>,
    warned_resolution_change: bool,
}

#[cfg(target_os = "macos")]
struct MacScreenFrame {
    width: u32,
    height: u32,
    capture_wall_time_us: u64,
    buffer: NativeBuffer,
}

#[cfg(target_os = "macos")]
struct MacScreenInput {
    _capturer: cxx::UniquePtr<webrtc_sys::macos_screen_capturer::ffi::MacosScreenCapturer>,
    frame_rx: mpsc::Receiver<MacScreenFrame>,
    pending_frame: Option<MacScreenFrame>,
    warned_resolution_change: bool,
}

#[cfg(target_os = "macos")]
struct MacScreenCallback {
    frame_tx: mpsc::Sender<MacScreenFrame>,
}

#[cfg(target_os = "macos")]
impl webrtc_sys::macos_screen_capturer::MacosScreenCapturerCallback for MacScreenCallback {
    fn on_capture_result(
        &mut self,
        result: Result<
            cxx::UniquePtr<webrtc_sys::macos_screen_capturer::ffi::MacosScreenFrame>,
            webrtc_sys::macos_screen_capturer::MacosScreenCaptureError,
        >,
    ) {
        match result {
            Ok(frame) => {
                let width = match u32::try_from(frame.width()) {
                    Ok(width) if width > 0 => width,
                    _ => {
                        log::warn!("Dropping empty macOS screen capture frame");
                        return;
                    }
                };
                let height = match u32::try_from(frame.height()) {
                    Ok(height) if height > 0 => height,
                    _ => {
                        log::warn!("Dropping empty macOS screen capture frame");
                        return;
                    }
                };
                let pixel_buffer = frame.pixel_buffer() as *mut std::ffi::c_void;
                if pixel_buffer.is_null() {
                    log::warn!("Dropping macOS screen capture frame without CVPixelBuffer");
                    return;
                }

                let capture_wall_time_us = unix_time_us_now();
                let buffer = unsafe {
                    // SAFETY: `MacosScreenFrame::pixel_buffer` returns a retained CVPixelBufferRef.
                    // `NativeBuffer::from_cv_pixel_buffer` transfers that retain into WebRTC.
                    NativeBuffer::from_cv_pixel_buffer(pixel_buffer)
                };
                let frame = MacScreenFrame { width, height, capture_wall_time_us, buffer };
                if let Err(err) = self.frame_tx.send(frame) {
                    log::debug!("macOS screen frame receiver dropped: {}", err);
                }
            }
            Err(webrtc_sys::macos_screen_capturer::MacosScreenCaptureError::Temporary) => {
                log::debug!("Temporary macOS screen capture error");
            }
            Err(webrtc_sys::macos_screen_capturer::MacosScreenCaptureError::Permanent) => {
                log::error!("Permanent macOS screen capture error");
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl MacScreenInput {
    fn new(screen_index: usize, fps: u32) -> Result<(u32, u32, Self)> {
        let mut capturer = webrtc_sys::macos_screen_capturer::ffi::new_macos_screen_capturer();
        if capturer.is_null() {
            bail!("Failed to create macOS screen capturer");
        }

        let screens = capturer.get_screen_list();
        let selected_screen = screens.get(screen_index).cloned().ok_or_else(|| {
            anyhow!(
                "Invalid screen index {}. Available macOS screens:\n{}",
                screen_index,
                format_macos_screens(&screens)
            )
        })?;
        info!(
            "Selected macOS native screen {}: {} ({}x{})",
            screen_index, selected_screen.title, selected_screen.width, selected_screen.height
        );
        info!(
            "Screen capture mode: zero-copy macOS native candidate \
             (ScreenCaptureKit CVPixelBuffer -> WebRTC NativeBuffer; no publisher CPU I420 copy)"
        );
        info!("macOS native screen capture requested frame rate: {} fps", fps);

        let (frame_tx, frame_rx) = mpsc::channel();
        let callback = webrtc_sys::macos_screen_capturer::MacosScreenCapturerCallbackWrapper::new(
            Box::new(MacScreenCallback { frame_tx }),
        );
        if !capturer.pin_mut().start(selected_screen.id, fps, Box::new(callback)) {
            bail!("Failed to start macOS native screen capture");
        }

        let mut input = Self {
            _capturer: capturer,
            frame_rx,
            pending_frame: None,
            warned_resolution_change: false,
        };
        let first_frame = input.wait_for_initial_frame()?;
        let width = first_frame.width;
        let height = first_frame.height;
        input.pending_frame = Some(first_frame);
        info!(
            "macOS native screen capture opened: {}x{} using CVPixelBuffer frames",
            width, height
        );
        Ok((width, height, input))
    }

    fn wait_for_initial_frame(&mut self) -> Result<MacScreenFrame> {
        let started_at = Instant::now();
        while started_at.elapsed() < Duration::from_secs(2) {
            if let Some(frame) = self.read_frame_with_timeout(Duration::from_millis(100))? {
                return Ok(frame);
            }
        }

        bail!("Timed out waiting for first macOS screen capture frame");
    }

    fn read_frame(&mut self) -> Result<Option<MacScreenFrame>> {
        if let Some(frame) = self.pending_frame.take() {
            return Ok(Some(frame));
        }

        self.read_frame_with_timeout(Duration::from_millis(500))
    }

    fn read_frame_with_timeout(&mut self, timeout: Duration) -> Result<Option<MacScreenFrame>> {
        let mut frame = match self.frame_rx.recv_timeout(timeout) {
            Ok(frame) => frame,
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("macOS screen capture frame channel disconnected")
            }
        };

        while let Ok(newer_frame) = self.frame_rx.try_recv() {
            frame = newer_frame;
        }

        Ok(Some(frame))
    }

    fn warn_resolution_change(
        &mut self,
        frame_width: u32,
        frame_height: u32,
        width: u32,
        height: u32,
    ) {
        if self.warned_resolution_change {
            return;
        }

        log::warn!(
            "macOS screen capture size changed from {}x{} to {}x{}; dropping resized frames",
            width,
            height,
            frame_width,
            frame_height
        );
        self.warned_resolution_change = true;
    }
}

impl ScreenInput {
    fn new(screen_index: usize, align_buffers_for_display: bool) -> Result<(u32, u32, Self)> {
        let mut options = DesktopCapturerOptions::new(DesktopCaptureSourceType::Screen);
        options.set_include_cursor(false);
        #[cfg(target_os = "macos")]
        options.set_sck_system_picker(false);

        let mut capturer = DesktopCapturer::new(options)
            .ok_or_else(|| anyhow!("Failed to create desktop capturer"))?;
        let sources = capturer.get_source_list();
        let selected_source = sources.get(screen_index).cloned().ok_or_else(|| {
            anyhow!(
                "Invalid screen index {}. Available screens:\n{}",
                screen_index,
                format_capture_sources(&sources)
            )
        })?;

        info!("Selected screen {}: {}", screen_index, selected_source);
        info!(
            "Screen capture mode: CPU desktop capture fallback \
             (DesktopFrame ARGB -> I420; publisher uses CPU-accessible pixels)"
        );
        if align_buffers_for_display {
            info!("Publisher preview enabled: CPU capture buffers are stride-aligned for display");
        }

        let (frame_tx, frame_rx) = mpsc::channel();
        capturer.start_capture(Some(selected_source), move |result| match result {
            Ok(frame) => {
                let capture_wall_time_us = unix_time_us_now();
                if let Some(frame) =
                    convert_desktop_frame(frame, capture_wall_time_us, align_buffers_for_display)
                {
                    if let Err(err) = frame_tx.send(frame) {
                        log::debug!("Screen frame receiver dropped: {}", err);
                    }
                }
            }
            Err(CaptureError::Temporary) => {
                log::debug!("Temporary screen capture error");
            }
            Err(CaptureError::Permanent) => {
                log::error!("Permanent screen capture error");
            }
        });

        let mut input =
            Self { capturer, frame_rx, pending_frame: None, warned_resolution_change: false };
        let first_frame = input.wait_for_initial_frame()?;
        let width = first_frame.width;
        let height = first_frame.height;
        input.pending_frame = Some(first_frame);
        info!("CPU desktop screen capture opened: {}x{}", width, height);
        Ok((width, height, input))
    }

    fn wait_for_initial_frame(&mut self) -> Result<ScreenFrame> {
        let started_at = Instant::now();
        while started_at.elapsed() < Duration::from_secs(2) {
            if let Some(frame) = self.capture_next_frame(Duration::from_millis(100))? {
                return Ok(frame);
            }
        }

        bail!("Timed out waiting for first screen capture frame");
    }

    fn read_frame(&mut self) -> Result<Option<ScreenFrame>> {
        if let Some(frame) = self.pending_frame.take() {
            return Ok(Some(frame));
        }

        self.capture_next_frame(Duration::from_millis(500))
    }

    fn capture_next_frame(&mut self, timeout: Duration) -> Result<Option<ScreenFrame>> {
        self.capturer.capture_frame();
        match self.frame_rx.recv_timeout(timeout) {
            Ok(frame) => Ok(Some(frame)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("Screen capture frame channel disconnected")
            }
        }
    }

    fn warn_resolution_change(
        &mut self,
        frame_width: u32,
        frame_height: u32,
        width: u32,
        height: u32,
    ) {
        if self.warned_resolution_change {
            return;
        }

        log::warn!(
            "Screen capture size changed from {}x{} to {}x{}; dropping resized frames",
            width,
            height,
            frame_width,
            frame_height
        );
        self.warned_resolution_change = true;
    }
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

fn format_capture_sources(sources: &[CaptureSource]) -> String {
    if sources.is_empty() {
        return "  (none)".to_string();
    }

    sources
        .iter()
        .enumerate()
        .map(|(i, source)| format!("  {}. {}", i, source))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(target_os = "macos")]
fn format_macos_screens(screens: &[webrtc_sys::macos_screen_capturer::ffi::MacosScreen]) -> String {
    if screens.is_empty() {
        return "  (none)".to_string();
    }

    screens
        .iter()
        .enumerate()
        .map(|(i, screen)| {
            format!(
                "  {}. {} (id {}, {}x{})",
                i, screen.title, screen.id, screen.width, screen.height
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn convert_desktop_frame(
    frame: DesktopFrame,
    capture_wall_time_us: u64,
    align_for_display: bool,
) -> Option<ScreenFrame> {
    let width = u32::try_from(frame.width()).ok()?;
    let height = u32::try_from(frame.height()).ok()?;
    if width == 0 || height == 0 {
        log::warn!("Dropping empty screen capture frame: {}x{}", width, height);
        return None;
    }

    let mut buffer = create_i420_buffer(width, height, align_for_display);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (data_y, data_u, data_v) = buffer.data_mut();

    yuv_helper::argb_to_i420(
        frame.data(),
        frame.stride(),
        data_y,
        stride_y,
        data_u,
        stride_u,
        data_v,
        stride_v,
        width as i32,
        height as i32,
    );

    Some(ScreenFrame { width, height, capture_wall_time_us, buffer })
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

fn open_screen_input(
    screen_index: usize,
    fps: u32,
    display_video: bool,
    burn_timestamp: bool,
) -> Result<(u32, u32, VideoInput, &'static str, TrackSource)> {
    #[cfg(target_os = "macos")]
    {
        if !burn_timestamp {
            info!(
                "Trying screen capture mode: zero-copy macOS native candidate \
                 (ScreenCaptureKit CVPixelBuffer -> WebRTC NativeBuffer)"
            );
            if display_video {
                info!(
                    "Publisher preview is enabled: publishing can stay native, \
                     but preview rendering will create an I420 display copy"
                );
            }
            match MacScreenInput::new(screen_index, fps) {
                Ok((width, height, screen_input)) => {
                    info!("Selected screen capture mode: macOS native CVPixelBuffer");
                    return Ok((
                        width,
                        height,
                        VideoInput::MacScreen(screen_input),
                        "screen_share",
                        TrackSource::Screenshare,
                    ));
                }
                Err(err) => {
                    log::warn!(
                        "macOS native screen capture unavailable ({}); selecting CPU desktop capture fallback",
                        err
                    );
                }
            }
        } else {
            log::warn!(
                "--burn-timestamp requires CPU-mutable I420 frames; selecting CPU desktop capture fallback"
            );
        }
    }

    info!("Selected screen capture mode: CPU desktop capture");
    let (width, height, screen_input) = ScreenInput::new(screen_index, display_video)
        .with_context(|| format!("Failed to open screen {}", screen_index))?;
    Ok((width, height, VideoInput::Screen(screen_input), "screen_share", TrackSource::Screenshare))
}

async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    if args.list_cameras {
        return list_cameras();
    }

    // LiveKit connection details
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LIVEKIT_URL must be provided via --url or env");
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LIVEKIT_API_KEY must be provided via --api-key or env");
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LIVEKIT_API_SECRET must be provided via --api-secret or env");

    if args.min_playout_delay.is_some() || args.max_playout_delay.is_some() {
        let twirp_host = normalize_twirp_host(&url);
        let room_client = RoomClient::with_api_key(&twirp_host, &api_key, &api_secret);
        info!(
            "Recreating room '{}' with playout delay min={:?} max={:?} ms",
            args.room_name, args.min_playout_delay, args.max_playout_delay
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
                args.min_playout_delay.unwrap_or_default(),
                args.max_playout_delay.unwrap_or_default(),
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
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.dynacast = true;

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

    let (width, height, video_input, track_name, track_source) = if args.test_pattern {
        let width = args.width;
        let height = args.height;
        let fps = args.fps;
        info!("Test pattern enabled: SMPTE 75% color bars at {}x{} @ {} fps", width, height, fps);
        (
            width,
            height,
            VideoInput::TestPattern(TestPattern::new(width, height)),
            "camera",
            TrackSource::Camera,
        )
    } else if let Some(screen_index) = args.screen_index {
        open_screen_input(screen_index, args.fps, args.display_video, args.burn_timestamp)?
    } else {
        // Setup camera
        let camera_index = args.camera_index.unwrap_or(0);
        let index = CameraIndex::Index(camera_index as u32);
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = Camera::new(index, requested)?;
        // Try raw YUYV first (cheaper than MJPEG), fall back to MJPEG
        let wanted = CameraFormat::new(
            Resolution::new(args.width, args.height),
            FrameFormat::YUYV,
            args.fps,
        );
        let mut using_fmt = "YUYV";
        if let Err(_) = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
            RequestedFormatType::Exact(wanted),
        )) {
            let alt = CameraFormat::new(
                Resolution::new(args.width, args.height),
                FrameFormat::MJPEG,
                args.fps,
            );
            using_fmt = "MJPEG";
            let _ = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
                RequestedFormatType::Exact(alt),
            ));
        }
        camera.open_stream()?;
        let fmt = camera.camera_format();
        let width = fmt.width();
        let height = fmt.height();
        let fps = fmt.frame_rate();
        let is_yuyv = fmt.format() == FrameFormat::YUYV;
        info!("Camera opened: {}x{} @ {} fps (format: {})", width, height, fps, using_fmt);
        debug!("Negotiated nokhwa CameraFormat: {:?}", fmt);
        info!(
            "Selected conversion path: {}",
            if is_yuyv { "YUYV->I420 (libyuv)" } else { "Auto (RGB24 or MJPEG)" }
        );
        (width, height, VideoInput::Camera { camera, is_yuyv }, "camera", TrackSource::Camera)
    };
    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track(track_name, RtcVideoSource::Native(rtc_source.clone()));

    // Choose requested codec and attempt to publish; if H.265 fails, retry with H.264
    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    // Compute an explicit video encoding so all simulcast layers use 30 fps.
    // The SDK defaults reduce lower layers to 15/20 fps; we override that here.
    let target_fps = args.fps as f64;
    let main_encoding = {
        let base = options::compute_appropriate_encoding(false, width, height, VideoCodec::H264);
        VideoEncoding {
            max_bitrate: args.max_bitrate.unwrap_or(base.max_bitrate),
            max_framerate: target_fps,
        }
    };
    let simulcast_presets = compute_simulcast_presets_30fps(width, height, target_fps);
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

    let mut packet_trailer_features = PacketTrailerFeatures::default();
    packet_trailer_features.user_timestamp = args.attach_timestamp;
    packet_trailer_features.frame_id = args.attach_frame_id;

    let publish_opts = |codec: VideoCodec| TrackPublishOptions {
        source: track_source,
        simulcast: args.simulcast,
        video_codec: codec,
        packet_trailer_features,
        video_encoding: Some(main_encoding.clone()),
        simulcast_layers: Some(simulcast_presets.clone()),
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
            info!("Published {} track with H.264 fallback", track_name);
            VideoCodec::H264
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published {} track", track_name);
        requested_codec
    };
    spawn_publisher_outbound_stats_logger(track.clone(), ctrl_c_received.clone());

    let capture_config = CaptureConfig {
        fps: args.fps,
        attach_timestamp: args.attach_timestamp,
        burn_timestamp: args.burn_timestamp,
        attach_frame_id: args.attach_frame_id,
        display_timing: args.display_timing,
    };

    if args.display_video {
        let shared = Arc::new(Mutex::new(SharedYuv {
            codec: actual_codec.as_str().to_ascii_uppercase(),
            ..Default::default()
        }));
        let capture_task = tokio::spawn(run_capture_loop(
            capture_config,
            ctrl_c_received.clone(),
            rtc_source,
            video_input,
            width,
            height,
            Some(shared.clone()),
        ));

        let display_result = video_display::run_display(
            "LiveKit Video Publisher",
            shared,
            ctrl_c_received.clone(),
            Some(width as f32 / height as f32),
        );

        let capture_result = capture_task.await?;
        display_result?;
        capture_result?;
    } else {
        run_capture_loop(
            capture_config,
            ctrl_c_received,
            rtc_source,
            video_input,
            width,
            height,
            None,
        )
        .await?;
    }

    Ok(())
}

async fn run_capture_loop(
    config: CaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    mut video_input: VideoInput,
    width: u32,
    height: u32,
    display_shared: Option<Arc<Mutex<SharedYuv>>>,
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
    let mut logged_sensor_ts_source = false;
    let mut logged_sensor_ts_missing = false;
    let mut frame_counter: u32 = 1;
    let mut timestamp_overlay = (config.attach_timestamp && config.burn_timestamp)
        .then(|| TimestampOverlay::new(width, height));
    let mut latency_display = LatencyDisplay::default();
    let align_buffers_for_display = display_shared.is_some();
    let stall_warn_after = Duration::from_secs_f64((target.as_secs_f64() * 3.0).max(0.25));
    let mut last_submitted_at: Option<Instant> = None;
    let mut screen_miss_streak: u32 = 0;
    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        // Wait until the scheduled next frame time
        let paced_wait_started_at = Instant::now();
        ticker.tick().await;
        let paced_wait_finished_at = Instant::now();

        let source_frame_started_at = Instant::now();
        let frame_wall_time_us = unix_time_us_now();
        let (
            capture_wall_time_us,
            read_wall_time_us,
            source_frame_acquired_at,
            decode_finished_at,
            convert_finished_at,
            used_decode_path,
            record_convert_timing,
            mut buffer,
        ) = match &mut video_input {
            VideoInput::TestPattern(pattern) => {
                let mut buffer = create_i420_buffer(width, height, align_buffers_for_display);
                let (stride_y, stride_u, stride_v) = buffer.strides();
                let (data_y, data_u, data_v) = buffer.data_mut();
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
                    PublishBuffer::I420(buffer),
                )
            }
            VideoInput::Camera { camera, is_yuyv } => {
                // Capture the frame as early as possible so the attached timestamp is
                // close to the camera acquisition point.
                let frame_buf = camera.frame()?;
                let read_wall_time_us = unix_time_us_now();
                let camera_frame_acquired_at = Instant::now();

                // Prefer the backend-provided sensor/PTS wallclock when available for
                // a more accurate capture-to-subscriber latency measurement.
                let capture_wall_time_us = match frame_buf.capture_timestamp() {
                    Some(d) => {
                        if !logged_sensor_ts_source {
                            info!("Using sensor capture_timestamp for user_timestamp");
                            logged_sensor_ts_source = true;
                        }
                        d.as_micros() as u64
                    }
                    None => {
                        if !logged_sensor_ts_missing {
                            log::warn!(
                                "Buffer::capture_timestamp() not available; falling back to system wall clock"
                            );
                            logged_sensor_ts_missing = true;
                        }
                        frame_wall_time_us
                    }
                };

                let mut buffer = create_i420_buffer(width, height, align_buffers_for_display);
                let (stride_y, stride_u, stride_v) = buffer.strides();
                let (data_y, data_u, data_v) = buffer.data_mut();

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
                    PublishBuffer::I420(buffer),
                )
            }
            VideoInput::Screen(screen) => {
                let Some(screen_frame) = screen.read_frame()? else {
                    screen_miss_streak = screen_miss_streak.saturating_add(1);
                    let since_last_submit = last_submitted_at
                        .map(|submitted_at| submitted_at.elapsed())
                        .unwrap_or_else(|| source_frame_started_at.elapsed());
                    if since_last_submit >= stall_warn_after {
                        log::warn!(
                            "Screen capture stall: no frame available for {:.1}ms (miss streak {}, waited {:.1}ms this tick)",
                            since_last_submit.as_secs_f64() * 1000.0,
                            screen_miss_streak,
                            source_frame_started_at.elapsed().as_secs_f64() * 1000.0,
                        );
                    } else {
                        log::debug!("No screen frame available before timeout; dropping tick");
                    }
                    continue;
                };
                if screen_miss_streak > 0 {
                    debug!("Screen capture recovered after {} missed ticks", screen_miss_streak);
                    screen_miss_streak = 0;
                }
                let read_wall_time_us = unix_time_us_now();
                let screen_frame_acquired_at = Instant::now();
                if screen_frame.width != width || screen_frame.height != height {
                    screen.warn_resolution_change(
                        screen_frame.width,
                        screen_frame.height,
                        width,
                        height,
                    );
                    continue;
                }

                let convert_finished_at = Instant::now();
                let capture_wall_time_us = screen_frame.capture_wall_time_us;
                let buffer = screen_frame.buffer;
                (
                    capture_wall_time_us,
                    read_wall_time_us,
                    screen_frame_acquired_at,
                    screen_frame_acquired_at,
                    convert_finished_at,
                    false,
                    true,
                    PublishBuffer::I420(buffer),
                )
            }
            #[cfg(target_os = "macos")]
            VideoInput::MacScreen(screen) => {
                let Some(screen_frame) = screen.read_frame()? else {
                    screen_miss_streak = screen_miss_streak.saturating_add(1);
                    let since_last_submit = last_submitted_at
                        .map(|submitted_at| submitted_at.elapsed())
                        .unwrap_or_else(|| source_frame_started_at.elapsed());
                    if since_last_submit >= stall_warn_after {
                        log::warn!(
                            "macOS screen capture stall: no frame available for {:.1}ms (miss streak {}, waited {:.1}ms this tick)",
                            since_last_submit.as_secs_f64() * 1000.0,
                            screen_miss_streak,
                            source_frame_started_at.elapsed().as_secs_f64() * 1000.0,
                        );
                    } else {
                        log::debug!(
                            "No macOS screen frame available before timeout; dropping tick"
                        );
                    }
                    continue;
                };
                if screen_miss_streak > 0 {
                    debug!(
                        "macOS screen capture recovered after {} missed ticks",
                        screen_miss_streak
                    );
                    screen_miss_streak = 0;
                }
                let read_wall_time_us = unix_time_us_now();
                let screen_frame_acquired_at = Instant::now();
                if screen_frame.width != width || screen_frame.height != height {
                    screen.warn_resolution_change(
                        screen_frame.width,
                        screen_frame.height,
                        width,
                        height,
                    );
                    continue;
                }

                let capture_wall_time_us = screen_frame.capture_wall_time_us;
                (
                    capture_wall_time_us,
                    read_wall_time_us,
                    screen_frame_acquired_at,
                    screen_frame_acquired_at,
                    screen_frame_acquired_at,
                    false,
                    false,
                    PublishBuffer::Native(screen_frame.buffer),
                )
            }
        };

        let fid = if config.attach_frame_id {
            let id = frame_counter;
            frame_counter = frame_counter.wrapping_add(1);
            Some(id)
        } else {
            None
        };
        let mut buffer_ready_at = convert_finished_at;
        let mut frame_draw_ms = None;
        let mut burned_timestamp_us = None;
        if let Some(overlay) = timestamp_overlay.as_mut() {
            let overlay_started_at = Instant::now();
            if let Some(buffer) = buffer.i420_mut() {
                let (stride_y, _, _) = buffer.strides();
                let stride_y_usize = stride_y as usize;
                let (data_y, _, _) = buffer.data_mut();
                overlay.draw(data_y, stride_y_usize, capture_wall_time_us, fid);
                burned_timestamp_us = Some(capture_wall_time_us);
                let overlay_finished_at = Instant::now();
                frame_draw_ms =
                    Some((overlay_finished_at - overlay_started_at).as_secs_f64() * 1000.0);
                buffer_ready_at = overlay_finished_at;
            }
        }

        // Build frame metadata from enabled packet trailer features
        let user_ts = if config.attach_timestamp { Some(capture_wall_time_us) } else { None };
        if burned_timestamp_us.is_some() {
            debug_assert_eq!(burned_timestamp_us, user_ts);
        }
        let frame_metadata = if user_ts.is_some() || fid.is_some() {
            Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
        } else {
            None
        };
        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            // Monotonic, microseconds since start.
            timestamp_us: start_ts.elapsed().as_micros() as i64,
            frame_metadata,
            buffer,
        };
        rtc_source.capture_frame(&frame);
        let sent_timestamp_us = unix_time_us_now();
        let webrtc_capture_finished_at = Instant::now();
        if let Some(previous_submit) = last_submitted_at.replace(webrtc_capture_finished_at) {
            let submit_gap = webrtc_capture_finished_at.saturating_duration_since(previous_submit);
            if submit_gap >= stall_warn_after {
                log::warn!(
                    "Publisher submit gap: {:.1}ms between frames (frame_id {:?}, source->submit {:.1}ms)",
                    submit_gap.as_secs_f64() * 1000.0,
                    fid,
                    (webrtc_capture_finished_at - source_frame_started_at).as_secs_f64() * 1000.0,
                );
            }
        }
        if let Some(shared) = display_shared.as_ref() {
            if let Some(display_buffer) = frame.buffer.i420_for_display() {
                let display_buffer = display_buffer.buffer();
                let (stride_y, stride_u, stride_v) = display_buffer.strides();
                let (data_y, data_u, data_v) = display_buffer.data();
                let (timing_sample, publish_latency_display) = if config.display_timing {
                    let timing_sample = PublisherTimingSample {
                        frame_id: fid,
                        capture_timestamp_us: capture_wall_time_us,
                        read_timestamp_us: read_wall_time_us,
                        sent_timestamp_us,
                    };
                    let publish_latency_display = latency_display.value(
                        Instant::now(),
                        Some(format_us_delta_ms(sent_timestamp_us, capture_wall_time_us)),
                    );
                    (Some(timing_sample), Some(publish_latency_display))
                } else {
                    (None, None)
                };
                video_display::pack_i420_into_shared(
                    shared,
                    width,
                    height,
                    data_y,
                    stride_y,
                    data_u,
                    stride_u,
                    data_v,
                    stride_v,
                    timing_sample,
                    publish_latency_display,
                );
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
            info!(
                "Video status: {}x{} | ~{:.1} fps | target {:.2} ms",
                width,
                height,
                fps_est,
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
