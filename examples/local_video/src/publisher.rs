use anyhow::Result;
use clap::Parser;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{
    self, video as video_presets, PacketTrailerFeatures, TrackPublishOptions, VideoCodec,
    VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
use livekit::webrtc::stats::RtcStats;
use livekit::webrtc::video_frame::{FrameMetadata, I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{debug, info};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
    Resolution,
};
use nokhwa::Camera;
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use yuv_sys;

mod timestamp_burn;

use timestamp_burn::TimestampOverlay;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// Camera index to use (numeric)
    #[arg(long, default_value_t = 0)]
    camera_index: usize,

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

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64
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

fn find_video_encoder_stats(stats: &[RtcStats]) -> Option<(&str, bool)> {
    stats.iter().find_map(|stat| match stat {
        RtcStats::OutboundRtp(outbound)
            if outbound.stream.kind == "video"
                && !outbound.outbound.encoder_implementation.is_empty() =>
        {
            Some((
                outbound.outbound.encoder_implementation.as_str(),
                outbound.outbound.power_efficient_encoder,
            ))
        }
        _ => None,
    })
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

    // Setup camera
    let index = CameraIndex::Index(args.camera_index as u32);
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, requested)?;
    // Try raw YUYV first (cheaper than MJPEG), fall back to MJPEG
    let wanted =
        CameraFormat::new(Resolution::new(args.width, args.height), FrameFormat::YUYV, args.fps);
    let mut using_fmt = "YUYV";
    if let Err(_) = camera
        .set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(wanted)))
    {
        let alt = CameraFormat::new(
            Resolution::new(args.width, args.height),
            FrameFormat::MJPEG,
            args.fps,
        );
        using_fmt = "MJPEG";
        let _ = camera
            .set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(alt)));
    }
    camera.open_stream()?;
    let fmt = camera.camera_format();
    let width = fmt.width();
    let height = fmt.height();
    let fps = fmt.frame_rate();
    info!("Camera opened: {}x{} @ {} fps (format: {})", width, height, fps, using_fmt);
    debug!("Negotiated nokhwa CameraFormat: {:?}", fmt);
    // Pace publishing at the requested FPS (not the camera-reported FPS) to hit desired cadence
    let pace_fps = args.fps as f64;

    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));

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
        source: TrackSource::Camera,
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

    if let Err(e) = publish_result {
        if matches!(requested_codec, VideoCodec::H265) {
            log::warn!("H.265 publish failed ({}). Falling back to H.264...", e);
            room.local_participant()
                .publish_track(LocalTrack::Video(track.clone()), publish_opts(VideoCodec::H264))
                .await?;
            info!("Published camera track with H.264 fallback");
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published camera track");
    }

    let stats_track = track.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        let mut last_encoder = String::new();
        loop {
            interval.tick().await;
            match stats_track.get_stats().await {
                Ok(stats) => {
                    if let Some((encoder, power_efficient)) = find_video_encoder_stats(&stats) {
                        if encoder != last_encoder {
                            info!(
                                "Video encoder implementation: {} (power efficient: {})",
                                encoder, power_efficient
                            );
                            last_encoder = encoder.to_owned();
                        }
                    }
                }
                Err(e) => debug!("Failed to get publisher video stats: {:?}", e),
            }
        }
    });

    // Reusable I420 buffer and frame
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        frame_metadata: None,
        buffer: I420Buffer::new(width, height),
    };
    let is_yuyv = fmt.format() == FrameFormat::YUYV;
    info!(
        "Selected conversion path: {}",
        if is_yuyv { "YUYV->I420 (libyuv)" } else { "Auto (RGB24 or MJPEG)" }
    );

    // Accurate pacing using absolute schedule (no drift)
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / pace_fps));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Align the first tick to now
    ticker.tick().await;
    let start_ts = Instant::now();

    // Capture loop
    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let target = Duration::from_secs_f64(1.0 / pace_fps);
    info!("Target frame interval: {:.2} ms", target.as_secs_f64() * 1000.0);

    // Timing accumulators (ms) for rolling stats
    let mut timings = PublisherTimingSummary::default();
    let mut logged_mjpeg_fallback = false;
    let mut logged_sensor_ts_source = false;
    let mut logged_sensor_ts_missing = false;
    let mut frame_counter: u32 = 1;
    let mut timestamp_overlay = (args.attach_timestamp && args.burn_timestamp)
        .then(|| TimestampOverlay::new(width, height));
    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        // Wait until the scheduled next frame time
        let paced_wait_started_at = Instant::now();
        ticker.tick().await;
        let paced_wait_finished_at = Instant::now();

        // Capture the frame as early as possible so the attached timestamp is
        // close to the camera acquisition point.
        let fallback_wall_time_us = unix_time_us_now();
        let camera_capture_started_at = Instant::now();
        let frame_buf = camera.frame()?;
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
                fallback_wall_time_us
            }
        };
        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        let stride_y_usize = stride_y as usize;
        let (decode_finished_at, convert_finished_at, used_decode_path) = if is_yuyv {
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

        let mut buffer_ready_at = convert_finished_at;
        let mut frame_draw_ms = None;
        if let Some(overlay) = timestamp_overlay.as_mut() {
            let overlay_started_at = Instant::now();
            overlay.draw(data_y, stride_y_usize, capture_wall_time_us);
            let overlay_finished_at = Instant::now();
            frame_draw_ms = Some((overlay_finished_at - overlay_started_at).as_secs_f64() * 1000.0);
            buffer_ready_at = overlay_finished_at;
        }

        // Update RTP timestamp (monotonic, microseconds since start)
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        // Build frame metadata from enabled packet trailer features
        let user_ts = if args.attach_timestamp { Some(capture_wall_time_us) } else { None };
        let fid = if args.attach_frame_id {
            let id = frame_counter;
            frame_counter = frame_counter.wrapping_add(1);
            Some(id)
        } else {
            None
        };
        frame.frame_metadata = if user_ts.is_some() || fid.is_some() {
            Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
        } else {
            None
        };
        rtc_source.capture_frame(&frame);
        let webrtc_capture_finished_at = Instant::now();

        frames += 1;

        // Per-iteration timing bookkeeping
        timings
            .paced_wait_ms
            .record((paced_wait_finished_at - paced_wait_started_at).as_secs_f64() * 1000.0);
        timings
            .camera_frame_read_ms
            .record((camera_frame_acquired_at - camera_capture_started_at).as_secs_f64() * 1000.0);
        if used_decode_path {
            timings
                .decode_mjpeg_ms
                .record((decode_finished_at - camera_frame_acquired_at).as_secs_f64() * 1000.0);
        }
        timings
            .buffer_convert_ms
            .record((convert_finished_at - decode_finished_at).as_secs_f64() * 1000.0);
        if let Some(frame_draw_ms) = frame_draw_ms {
            timings.frame_draw_ms.record(frame_draw_ms);
        }
        timings
            .submit_to_webrtc_ms
            .record((webrtc_capture_finished_at - buffer_ready_at).as_secs_f64() * 1000.0);
        timings.capture_to_webrtc_total_ms.record(
            (webrtc_capture_finished_at - camera_capture_started_at).as_secs_f64() * 1000.0,
        );

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
