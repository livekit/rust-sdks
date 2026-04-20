use anyhow::Result;
use clap::Parser;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{debug, info, warn};
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
use std::time::{Duration, Instant};
use yuv_sys;

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
}

fn list_cameras() -> Result<()> {
    let cams = nokhwa::query(ApiBackend::Auto)?;
    println!("Available cameras:");
    for (i, cam) in cams.iter().enumerate() {
        println!("{}. {}", i, cam.human_name());
    }
    Ok(())
}

/// Try to open a camera with the given format, returning the Camera and the format name on success.
fn try_open_camera(
    index: &CameraIndex,
    width: u32,
    height: u32,
    fps: u32,
    formats_to_try: &[FrameFormat],
) -> Result<(Camera, String)> {
    // First, create the camera with a permissive initial request
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index.clone(), requested)?;

    for fmt in formats_to_try {
        let fmt_name = match fmt {
            FrameFormat::YUYV => "YUYV",
            FrameFormat::MJPEG => "MJPEG",
            _ => "Unknown",
        };
        let wanted = CameraFormat::new(Resolution::new(width, height), *fmt, fps);
        debug!("Trying camera format: {} {}x{} @ {} fps", fmt_name, width, height, fps);

        if camera
            .set_camera_requset(RequestedFormat::new::<RgbFormat>(
                RequestedFormatType::Exact(wanted),
            ))
            .is_ok()
        {
            match camera.open_stream() {
                Ok(()) => {
                    // Verify we can actually grab a frame before committing
                    match camera.frame() {
                        Ok(_) => {
                            let negotiated = camera.camera_format();
                            info!(
                                "Camera opened: {}x{} @ {} fps (format: {}, negotiated: {:?})",
                                negotiated.width(),
                                negotiated.height(),
                                negotiated.frame_rate(),
                                fmt_name,
                                negotiated
                            );
                            return Ok((camera, fmt_name.to_string()));
                        }
                        Err(e) => {
                            warn!(
                                "Camera format {} opened stream but frame() failed: {} — trying next format",
                                fmt_name, e
                            );
                            // Stop the stream before trying another format
                            let _ = camera.stop_stream();
                        }
                    }
                }
                Err(e) => {
                    warn!("Camera format {} open_stream failed: {} — trying next format", fmt_name, e);
                }
            }
        } else {
            debug!("Camera rejected format {}", fmt_name);
        }
    }

    // Last resort: try with no specific format constraint
    info!("All specific formats failed; trying unconstrained camera open...");
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index.clone(), requested)?;
    camera.open_stream()?;
    // Verify frame capture works
    camera.frame().map_err(|e| {
        anyhow::anyhow!(
            "Camera frame capture failed with all format attempts. \
             Last error: {}. \
             On Raspberry Pi, ensure you are using a compatible camera stack: \
             - For USB cameras: the V4L2 driver should work directly \
             - For CSI cameras: you may need the legacy camera stack (bcm2835-v4l2) \
               or use libcamera tools. Add 'start_x=1' and 'gpu_mem=128' to /boot/config.txt \
               and remove 'camera_auto_detect=1' if present.",
            e
        )
    })?;
    let negotiated = camera.camera_format();
    let fmt_name = format!("{:?}", negotiated.format());
    info!(
        "Camera opened (unconstrained): {}x{} @ {} fps (format: {})",
        negotiated.width(),
        negotiated.height(),
        negotiated.frame_rate(),
        fmt_name
    );
    Ok((camera, fmt_name))
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
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = std::sync::Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

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

    // Setup camera — try YUYV first (cheaper), then MJPEG, with frame-capture verification.
    // On Raspberry Pi with libcamera, some formats may negotiate successfully but fail at
    // frame capture time (EPROTO / "Protocol error"). The try_open_camera helper handles this
    // by testing an actual frame grab before committing to a format.
    let index = CameraIndex::Index(args.camera_index as u32);
    let (mut camera, _using_fmt) = try_open_camera(
        &index,
        args.width,
        args.height,
        args.fps,
        &[FrameFormat::YUYV, FrameFormat::MJPEG],
    )?;
    let fmt = camera.camera_format();
    let width = fmt.width();
    let height = fmt.height();
    let _fps = fmt.frame_rate();
    debug!("Final negotiated nokhwa CameraFormat: {:?}", fmt);
    // Pace publishing at the requested FPS (not the camera-reported FPS) to hit desired cadence
    let pace_fps = args.fps as f64;

    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));

    // Choose requested codec and attempt to publish; if H.265 fails, retry with H.264
    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    let publish_opts = |codec: VideoCodec| {
        let mut opts = TrackPublishOptions {
            source: TrackSource::Camera,
            simulcast: args.simulcast,
            video_codec: codec,
            ..Default::default()
        };
        if let Some(bitrate) = args.max_bitrate {
            opts.video_encoding =
                Some(VideoEncoding { max_bitrate: bitrate, max_framerate: args.fps as f64 });
        }
        opts
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

    // Reusable I420 buffer and frame
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
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
    let mut sum_get_ms = 0.0;
    let mut sum_decode_ms = 0.0;
    let mut sum_convert_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut sum_sleep_ms = 0.0;
    let mut sum_iter_ms = 0.0;
    let mut logged_mjpeg_fallback = false;
    let mut consecutive_frame_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 30; // ~1 second at 30fps before giving up
    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        // Wait until the scheduled next frame time
        let wait_start = Instant::now();
        ticker.tick().await;
        let iter_start = Instant::now();

        // Get frame as RGB24 (decoded by nokhwa if needed)
        let t0 = Instant::now();
        let frame_buf = match camera.frame() {
            Ok(buf) => {
                consecutive_frame_errors = 0;
                buf
            }
            Err(e) => {
                consecutive_frame_errors += 1;
                if consecutive_frame_errors == 1 {
                    warn!("Frame capture error: {} (will retry)", e);
                } else if consecutive_frame_errors % 10 == 0 {
                    warn!(
                        "Frame capture error ({}x consecutive): {}",
                        consecutive_frame_errors, e
                    );
                }
                if consecutive_frame_errors >= MAX_CONSECUTIVE_ERRORS {
                    return Err(anyhow::anyhow!(
                        "Camera frame capture failed {} times consecutively: {}. \
                         On Raspberry Pi, this often means the camera format is incompatible \
                         with the V4L2/libcamera stack. Try: \
                         1) A USB camera instead of CSI \
                         2) Different resolution (e.g. --width 640 --height 480) \
                         3) Enabling the legacy camera stack in /boot/config.txt",
                        consecutive_frame_errors,
                        e
                    ));
                }
                continue;
            }
        };
        let t1 = Instant::now();
        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        // Fast path for YUYV: convert directly to I420 via libyuv
        let t2 = if is_yuyv {
            let src = frame_buf.buffer();
            let src_bytes = src.as_ref();
            let src_stride = (width * 2) as i32; // YUYV packed 4:2:2
            let t2_local = t1; // no decode step in YUYV path
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
            t2_local
        } else {
            // Auto path (either RGB24 already or compressed MJPEG)
            let src = frame_buf.buffer();
            let t2_local = if src.len() == (width as usize * height as usize * 3) {
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
                Instant::now()
            } else {
                // Try fast MJPEG->I420 via libyuv if available; fallback to image crate
                let mut used_fast_mjpeg = false;
                let t2_try = unsafe {
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
                        t1
                    }
                };
                if used_fast_mjpeg {
                    t2_try
                } else {
                    // Fallback: decode MJPEG using image crate then RGB24->I420
                    match image::load_from_memory(src.as_ref()) {
                        Ok(img_dyn) => {
                            let rgb8 = img_dyn.to_rgb8();
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
                            Instant::now()
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
            };
            t2_local
        };
        let t3 = Instant::now();

        // Update RTP timestamp (monotonic, microseconds since start)
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
        rtc_source.capture_frame(&frame);
        let t4 = Instant::now();

        frames += 1;
        // We already paced via interval; measure actual sleep time for logging only
        let sleep_dur = iter_start - wait_start;

        // Per-iteration timing bookkeeping
        let t_end = Instant::now();
        let get_ms = (t1 - t0).as_secs_f64() * 1000.0;
        let decode_ms = (t2 - t1).as_secs_f64() * 1000.0;
        let convert_ms = (t3 - t2).as_secs_f64() * 1000.0;
        let capture_ms = (t4 - t3).as_secs_f64() * 1000.0;
        let sleep_ms = sleep_dur.as_secs_f64() * 1000.0;
        let iter_ms = (t_end - iter_start).as_secs_f64() * 1000.0;
        sum_get_ms += get_ms;
        sum_decode_ms += decode_ms;
        sum_convert_ms += convert_ms;
        sum_capture_ms += capture_ms;
        sum_sleep_ms += sleep_ms;
        sum_iter_ms += iter_ms;

        if last_fps_log.elapsed() >= std::time::Duration::from_secs(2) {
            let secs = last_fps_log.elapsed().as_secs_f64();
            let fps_est = frames as f64 / secs;
            let n = frames.max(1) as f64;
            info!(
                "Publishing video: {}x{}, ~{:.1} fps | avg ms: get {:.2}, decode {:.2}, convert {:.2}, capture {:.2}, sleep {:.2}, iter {:.2} | target {:.2}",
                width,
                height,
                fps_est,
                sum_get_ms / n,
                sum_decode_ms / n,
                sum_convert_ms / n,
                sum_capture_ms / n,
                sum_sleep_ms / n,
                sum_iter_ms / n,
                target.as_secs_f64() * 1000.0,
            );
            frames = 0;
            sum_get_ms = 0.0;
            sum_decode_ms = 0.0;
            sum_convert_ms = 0.0;
            sum_capture_ms = 0.0;
            sum_sleep_ms = 0.0;
            sum_iter_ms = 0.0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}
