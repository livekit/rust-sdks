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
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
    Camera { camera: Camera, is_yuyv: bool },
    Screen(ScreenInput),
}

struct ScreenFrame {
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    data_y: Vec<u8>,
    data_u: Vec<u8>,
    data_v: Vec<u8>,
}

struct ScreenInput {
    capturer: DesktopCapturer,
    frame_rx: mpsc::Receiver<ScreenFrame>,
    pending_frame: Option<ScreenFrame>,
    warned_resolution_change: bool,
}

impl ScreenInput {
    fn new(screen_index: usize) -> Result<(u32, u32, Self)> {
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

        let (frame_tx, frame_rx) = mpsc::channel();
        capturer.start_capture(Some(selected_source), move |result| match result {
            Ok(frame) => {
                if let Some(frame) = convert_desktop_frame(frame) {
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
        info!("Screen capture opened: {}x{}", width, height);
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

fn convert_desktop_frame(frame: DesktopFrame) -> Option<ScreenFrame> {
    let width = u32::try_from(frame.width()).ok()?;
    let height = u32::try_from(frame.height()).ok()?;
    if width == 0 || height == 0 {
        log::warn!("Dropping empty screen capture frame: {}x{}", width, height);
        return None;
    }

    let stride_y = width;
    let stride_u = (width + 1) / 2;
    let stride_v = stride_u;
    let chroma_height = (height + 1) / 2;
    let mut data_y = vec![0; (stride_y * height) as usize];
    let mut data_u = vec![0; (stride_u * chroma_height) as usize];
    let mut data_v = vec![0; (stride_v * chroma_height) as usize];

    yuv_helper::argb_to_i420(
        frame.data(),
        frame.stride(),
        &mut data_y,
        stride_y,
        &mut data_u,
        stride_u,
        &mut data_v,
        stride_v,
        width as i32,
        height as i32,
    );

    Some(ScreenFrame { width, height, stride_y, stride_u, stride_v, data_y, data_u, data_v })
}

fn copy_plane(
    src: &[u8],
    src_stride: u32,
    dst: &mut [u8],
    dst_stride: u32,
    row_bytes: u32,
    rows: u32,
) {
    let src_stride = src_stride as usize;
    let dst_stride = dst_stride as usize;
    let row_bytes = row_bytes as usize;

    for row in 0..rows as usize {
        let src_start = row * src_stride;
        let dst_start = row * dst_stride;
        dst[dst_start..dst_start + row_bytes]
            .copy_from_slice(&src[src_start..src_start + row_bytes]);
    }
}

fn copy_i420_frame(
    src: &ScreenFrame,
    dst_y: &mut [u8],
    dst_stride_y: u32,
    dst_u: &mut [u8],
    dst_stride_u: u32,
    dst_v: &mut [u8],
    dst_stride_v: u32,
) {
    let chroma_width = (src.width + 1) / 2;
    let chroma_height = (src.height + 1) / 2;
    copy_plane(&src.data_y, src.stride_y, dst_y, dst_stride_y, src.width, src.height);
    copy_plane(&src.data_u, src.stride_u, dst_u, dst_stride_u, chroma_width, chroma_height);
    copy_plane(&src.data_v, src.stride_v, dst_v, dst_stride_v, chroma_width, chroma_height);
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
        let (width, height, screen_input) = ScreenInput::new(screen_index)
            .with_context(|| format!("Failed to open screen {}", screen_index))?;
        (width, height, VideoInput::Screen(screen_input), "screen_share", TrackSource::Screenshare)
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
        let mut buffer = create_i420_buffer(width, height, align_buffers_for_display);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data_mut();
        let stride_y_usize = stride_y as usize;
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
            VideoInput::Screen(screen) => {
                let Some(screen_frame) = screen.read_frame()? else {
                    log::debug!("No screen frame available before timeout; dropping tick");
                    continue;
                };
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

                copy_i420_frame(
                    &screen_frame,
                    data_y,
                    stride_y,
                    data_u,
                    stride_u,
                    data_v,
                    stride_v,
                );
                let convert_finished_at = Instant::now();
                (
                    frame_wall_time_us,
                    read_wall_time_us,
                    screen_frame_acquired_at,
                    screen_frame_acquired_at,
                    convert_finished_at,
                    false,
                    true,
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
            overlay.draw(data_y, stride_y_usize, capture_wall_time_us, fid);
            burned_timestamp_us = Some(capture_wall_time_us);
            let overlay_finished_at = Instant::now();
            frame_draw_ms = Some((overlay_finished_at - overlay_started_at).as_secs_f64() * 1000.0);
            buffer_ready_at = overlay_finished_at;
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
        if let Some(shared) = display_shared.as_ref() {
            let (stride_y, stride_u, stride_v) = frame.buffer.strides();
            let (data_y, data_u, data_v) = frame.buffer.data();
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
                stride_y as u32,
                data_u,
                stride_u as u32,
                data_v,
                stride_v as u32,
                timing_sample,
                publish_latency_display,
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
