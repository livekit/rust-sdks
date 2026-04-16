use anyhow::Result;
use clap::Parser;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use libwebrtc::stats::RtcStats;
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
use std::time::{Duration, Instant};
use yuv_sys;

#[path = "test_pattern.rs"]
mod test_pattern;
use test_pattern::{
    fill_color_bars, parse_video_source, render_timecode_overlay, VideoSourceMode,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// Camera index (number), or 'static'/'timecode' for test patterns
    #[arg(long, default_value = "0")]
    camera_index: String,

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

    let source_mode = parse_video_source(&args.camera_index)?;

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

    // Determine frame dimensions (and optionally open camera)
    let width;
    let height;
    let mut camera_state: Option<(Camera, bool)> = None;

    match &source_mode {
        VideoSourceMode::Camera(idx) => {
            let index = CameraIndex::Index(*idx);
            let requested =
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            let mut camera = Camera::new(index, requested)?;
            let wanted = CameraFormat::new(
                Resolution::new(args.width, args.height),
                FrameFormat::YUYV,
                args.fps,
            );
            let mut using_fmt = "YUYV";
            if camera
                .set_camera_requset(RequestedFormat::new::<RgbFormat>(
                    RequestedFormatType::Exact(wanted),
                ))
                .is_err()
            {
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
            width = fmt.width();
            height = fmt.height();
            let is_yuyv = fmt.format() == FrameFormat::YUYV;
            info!(
                "Camera opened: {}x{} @ {} fps (format: {})",
                width,
                height,
                fmt.frame_rate(),
                using_fmt
            );
            debug!("Negotiated nokhwa CameraFormat: {:?}", fmt);
            camera_state = Some((camera, is_yuyv));
        }
        _ => {
            width = args.width;
            height = args.height;
            info!(
                "Test pattern mode ({:?}): {}x{} @ {} fps",
                source_mode, width, height, args.fps
            );
        }
    }

    let pace_fps = args.fps as f64;

    // Create LiveKit video source and track.
    // For test patterns, mark the source as screencast so WebRTC's AdaptedVideoTrackSource
    // preserves the full resolution rather than scaling it down via AdaptFrame.
    let is_screencast = !matches!(source_mode, VideoSourceMode::Camera(_));
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, is_screencast);
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
            info!("Published track with H.264 fallback");
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published track");
    }

    // Periodically log encoder details
    {
        let track = track.clone();
        let ctrl_c_stats = ctrl_c_received.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.tick().await;
            loop {
                if ctrl_c_stats.load(Ordering::Acquire) {
                    break;
                }
                match track.get_stats().await {
                    Ok(stats) => {
                        for stat in &stats {
                            if let RtcStats::OutboundRtp(rtp) = stat {
                                let enc = &rtp.outbound.encoder_implementation;
                                if enc.is_empty() {
                                    continue;
                                }
                                let hw_sw = if rtp.outbound.power_efficient_encoder {
                                    "HW"
                                } else {
                                    "SW"
                                };
                                let codec = stats
                                    .iter()
                                    .find_map(|s| match s {
                                        RtcStats::Codec(c)
                                            if c.rtc.id == rtp.stream.codec_id =>
                                        {
                                            Some(c.codec.mime_type.as_str())
                                        }
                                        _ => None,
                                    })
                                    .unwrap_or("?");
                                let rid = if rtp.outbound.rid.is_empty() {
                                    "-"
                                } else {
                                    &rtp.outbound.rid
                                };
                                info!(
                                    "Encoder [{}]: {} ({}, {}) — {}x{} @ {:.1} fps, target {:.0} kbps",
                                    rid,
                                    enc,
                                    hw_sw,
                                    codec,
                                    rtp.outbound.frame_width,
                                    rtp.outbound.frame_height,
                                    rtp.outbound.frames_per_second,
                                    rtp.outbound.target_bitrate / 1000.0,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Could not retrieve encoder stats: {}", e);
                    }
                }
                interval.tick().await;
            }
        });
    }

    // Reusable I420 buffer and frame
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        buffer: I420Buffer::new(width, height),
    };

    // Accurate pacing using absolute schedule (no drift)
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / pace_fps));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    let start_ts = Instant::now();

    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let target = Duration::from_secs_f64(1.0 / pace_fps);

    match &source_mode {
        // -----------------------------------------------------------------
        // Camera capture loop
        // -----------------------------------------------------------------
        VideoSourceMode::Camera(_) => {
            let (mut camera, is_yuyv) = camera_state.unwrap();
            info!(
                "Selected conversion path: {}",
                if is_yuyv { "YUYV->I420 (libyuv)" } else { "Auto (RGB24 or MJPEG)" }
            );
            info!("Target frame interval: {:.2} ms", target.as_secs_f64() * 1000.0);

            let mut sum_get_ms = 0.0;
            let mut sum_decode_ms = 0.0;
            let mut sum_convert_ms = 0.0;
            let mut sum_capture_ms = 0.0;
            let mut sum_sleep_ms = 0.0;
            let mut sum_iter_ms = 0.0;
            let mut logged_mjpeg_fallback = false;

            loop {
                if ctrl_c_received.load(Ordering::Acquire) {
                    break;
                }
                let wait_start = Instant::now();
                ticker.tick().await;
                let iter_start = Instant::now();

                let t0 = Instant::now();
                let frame_buf = camera.frame()?;
                let t1 = Instant::now();
                let (stride_y, stride_u, stride_v) = frame.buffer.strides();
                let (data_y, data_u, data_v) = frame.buffer.data_mut();
                // Fast path for YUYV: convert directly to I420 via libyuv
                let t2 = if is_yuyv {
                    let src = frame_buf.buffer();
                    let src_bytes = src.as_ref();
                    let src_stride = (width * 2) as i32;
                    unsafe {
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
                    t1
                } else {
                    // Auto path (either RGB24 already or compressed MJPEG)
                    let src = frame_buf.buffer();
                    let t2_local = if src.len() == (width as usize * height as usize * 3) {
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
                        let mut used_fast_mjpeg = false;
                        let t2_try = unsafe {
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

                frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
                rtc_source.capture_frame(&frame);
                let t4 = Instant::now();

                frames += 1;
                let sleep_dur = iter_start - wait_start;
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

                if last_fps_log.elapsed() >= Duration::from_secs(2) {
                    let secs = last_fps_log.elapsed().as_secs_f64();
                    let fps_est = frames as f64 / secs;
                    let n = frames.max(1) as f64;
                    info!(
                        "Publishing video: {}x{}, ~{:.1} fps | avg ms: get {:.2}, decode {:.2}, convert {:.2}, capture {:.2}, sleep {:.2}, iter {:.2} | target {:.2}",
                        width, height, fps_est,
                        sum_get_ms / n, sum_decode_ms / n, sum_convert_ms / n,
                        sum_capture_ms / n, sum_sleep_ms / n, sum_iter_ms / n,
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
        }

        // -----------------------------------------------------------------
        // Test-pattern capture loop (static or timecode)
        // -----------------------------------------------------------------
        mode => {
            // For the static pattern, fill the buffer once and reuse every frame
            // (identical content → encoder sees zero motion).
            if matches!(mode, VideoSourceMode::Static) {
                let (stride_y, stride_u, stride_v) = frame.buffer.strides();
                let (y_data, u_data, v_data) = frame.buffer.data_mut();
                fill_color_bars(
                    y_data, u_data, v_data, stride_y, stride_u, stride_v, width, height,
                );
            }

            loop {
                if ctrl_c_received.load(Ordering::Acquire) {
                    break;
                }
                ticker.tick().await;

                if matches!(mode, VideoSourceMode::Timecode) {
                    let (stride_y, stride_u, stride_v) = frame.buffer.strides();
                    let (y_data, u_data, v_data) = frame.buffer.data_mut();
                    fill_color_bars(
                        y_data, u_data, v_data, stride_y, stride_u, stride_v, width, height,
                    );
                    render_timecode_overlay(
                        y_data, u_data, v_data, stride_y, stride_u, stride_v, width, height,
                    );
                }

                frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
                rtc_source.capture_frame(&frame);
                frames += 1;

                if last_fps_log.elapsed() >= Duration::from_secs(2) {
                    let secs = last_fps_log.elapsed().as_secs_f64();
                    let fps_est = frames as f64 / secs;
                    info!(
                        "Publishing {:?} pattern: {}x{}, ~{:.1} fps",
                        mode, width, height, fps_est
                    );
                    frames = 0;
                    last_fps_log = Instant::now();
                }
            }
        }
    }

    Ok(())
}
