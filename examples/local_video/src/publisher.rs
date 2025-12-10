use anyhow::Result;
use clap::Parser;
use eframe::egui;
use egui_wgpu as egui_wgpu_backend;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{debug, info};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use parking_lot::Mutex;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use yuv_sys as yuv_sys;

mod yuv_viewer;
use yuv_viewer::{SharedYuv, YuvPaintCallback};

fn format_sensor_timestamp(ts_micros: i64) -> Option<String> {
    if ts_micros == 0 {
        // Treat 0 as "not set"
        return None;
    }
    let nanos = i128::from(ts_micros).checked_mul(1_000)?;
    let dt = time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
    let format = time::macros::format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second]:[subsecond digits:3]"
    );
    dt.format(&format).ok()
}

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

    /// Shared E2EE key (enables end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,

    /// Attach sensor timestamps to published frames (for testing)
    #[arg(long, default_value_t = false)]
    sensor_timestamp: bool,

    /// Use H.265/HEVC encoding if supported (falls back to H.264 on failure)
    #[arg(long, default_value_t = false)]
    h265: bool,

    /// Show a local preview window for the captured video
    #[arg(long, default_value_t = false)]
    show_video: bool,
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

    if args.list_cameras {
        return list_cameras();
    }

    // LiveKit connection details
    let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok()).expect(
        "LIVEKIT_URL must be provided via --url or env",
    );
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
    if let Some(ref key) = args.e2ee_key {
        let key_provider =
            KeyProvider::with_shared_key(KeyProviderOptions::default(), key.clone().into_bytes());
        room_options.encryption =
            Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
        info!("E2EE enabled with provided shared key");
    }
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

    // Setup camera
    let index = CameraIndex::Index(args.camera_index as u32);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, requested)?;
    // Try raw YUYV first (cheaper than MJPEG), fall back to MJPEG
    let wanted = CameraFormat::new(
        Resolution::new(args.width, args.height),
        FrameFormat::YUYV,
        args.fps,
    );
    let mut using_fmt = "YUYV";
    if let Err(_) = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(wanted))) {
        let alt = CameraFormat::new(
            Resolution::new(args.width, args.height),
            FrameFormat::MJPEG,
            args.fps,
        );
        using_fmt = "MJPEG";
        let _ = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(alt)));
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
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height });
    let track = LocalVideoTrack::create_video_track(
        "camera",
        RtcVideoSource::Native(rtc_source.clone()),
    );

    // Choose requested codec and attempt to publish; if H.265 fails, retry with H.264
    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    let publish_opts = |codec: VideoCodec| {
        let mut opts = TrackPublishOptions {
            source: TrackSource::Camera,
            simulcast: false,
            video_codec: codec,
            ..Default::default()
        };
        if let Some(bitrate) = args.max_bitrate {
            opts.video_encoding = Some(VideoEncoding {
                max_bitrate: bitrate,
                max_framerate: args.fps as f64,
            });
        }
        opts
    };

    let publish_result = room
        .local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_opts(requested_codec))
        .await;

    if let Err(e) = publish_result {
        if matches!(requested_codec, VideoCodec::H265) {
            log::warn!(
                "H.265 publish failed ({}). Falling back to H.264...",
                e
            );
            room
                .local_participant()
                .publish_track(LocalTrack::Video(track.clone()), publish_opts(VideoCodec::H264))
                .await?;
            info!("Published camera track with H.264 fallback");
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published camera track");
    }

    // Optional shared YUV buffer for local preview UI
    let shared_preview = if args.show_video {
        Some(Arc::new(Mutex::new(SharedYuv {
            width: 0,
            height: 0,
            stride_y: 0,
            stride_u: 0,
            stride_v: 0,
            y: Vec::new(),
            u: Vec::new(),
            v: Vec::new(),
            dirty: false,
            sensor_timestamp: None,
        })))
    } else {
        None
    };

    // Spawn the capture loop on the Tokio runtime so we can optionally run an egui
    // preview window on the main thread.
    let capture_shared = shared_preview.clone();
    let show_sensor_ts = args.sensor_timestamp;
    let capture_handle = tokio::spawn(async move {
        // Reusable I420 buffer and frame
        let mut frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            sensor_timestamp_us: None,
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

        // Local YUV buffers reused for preview upload (if enabled)
        let mut y_buf: Vec<u8> = Vec::new();
        let mut u_buf: Vec<u8> = Vec::new();
        let mut v_buf: Vec<u8> = Vec::new();
        let mut last_sensor_ts: Option<i64> = None;

        loop {
            // Wait until the scheduled next frame time
            let wait_start = Instant::now();
            ticker.tick().await;
            let iter_start = Instant::now();

            // Get frame as RGB24 (decoded by nokhwa if needed)
            let t0 = Instant::now();
            let frame_buf = camera.frame()?;
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

            // Optionally attach a sensor timestamp and push it into the shared queue
            // used by the sensor timestamp transformer.
            if show_sensor_ts {
                if let Some(store) = track.sensor_timestamp_store() {
                    let sensor_ts = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .expect("SystemTime before UNIX EPOCH")
                        .as_micros() as i64;
                    frame.sensor_timestamp_us = Some(sensor_ts);
                    store.store(frame.timestamp_us, sensor_ts);
                    last_sensor_ts = Some(sensor_ts);
                    info!(
                        "Publisher: attached sensor_timestamp_us={} for capture_ts={}",
                        sensor_ts, frame.timestamp_us
                    );
                }
            }

            // If preview is enabled, copy I420 planes into the shared buffer.
            if let Some(shared) = &capture_shared {
                let (sy, su, sv) = (stride_y as u32, stride_u as u32, stride_v as u32);
                let (dy, du, dv) = frame.buffer.data();
                let ch = (height + 1) / 2;
                let y_size = (sy * height) as usize;
                let u_size = (su * ch) as usize;
                let v_size = (sv * ch) as usize;
                if y_buf.len() != y_size {
                    y_buf.resize(y_size, 0);
                }
                if u_buf.len() != u_size {
                    u_buf.resize(u_size, 0);
                }
                if v_buf.len() != v_size {
                    v_buf.resize(v_size, 0);
                }
                y_buf.copy_from_slice(dy);
                u_buf.copy_from_slice(du);
                v_buf.copy_from_slice(dv);

                let mut s = shared.lock();
                s.width = width;
                s.height = height;
                s.stride_y = sy;
                s.stride_u = su;
                s.stride_v = sv;
                std::mem::swap(&mut s.y, &mut y_buf);
                std::mem::swap(&mut s.u, &mut u_buf);
                std::mem::swap(&mut s.v, &mut v_buf);
                s.dirty = true;
                s.sensor_timestamp = last_sensor_ts;
            }

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
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    // If preview is requested, run an egui window on the main thread rendering from
    // the shared YUV buffer. Otherwise, just wait for the capture loop.
    if let Some(shared) = shared_preview {
        struct PreviewApp {
            shared: Arc<Mutex<SharedYuv>>,
        }

        impl eframe::App for PreviewApp {
            fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let available = ui.available_size();
                    let rect = egui::Rect::from_min_size(ui.min_rect().min, available);

                    ui.ctx().request_repaint();

                    let cb = egui_wgpu_backend::Callback::new_paint_callback(
                        rect,
                        YuvPaintCallback {
                            shared: self.shared.clone(),
                        },
                    );
                    ui.painter().add(cb);
                });

                // Sensor timestamp overlay: top-left, same style as subscriber.
                let sensor_timestamp_text = {
                    let shared = self.shared.lock();
                    shared
                        .sensor_timestamp
                        .and_then(format_sensor_timestamp)
                };
                if let Some(ts_text) = sensor_timestamp_text {
                    egui::Area::new("publisher_sensor_timestamp_overlay".into())
                        .anchor(egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0))
                        .interactable(false)
                        .show(ctx, |ui| {
                            ui.label(
                                egui::RichText::new(ts_text)
                                    .monospace()
                                    .size(22.0)
                                    .color(egui::Color32::WHITE),
                            );
                        });
                }

                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }

        let app = PreviewApp { shared };
        let native_options = eframe::NativeOptions::default();
        eframe::run_native(
            "LiveKit Camera Publisher Preview",
            native_options,
            Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))),
        )?;
        // When the window closes, main will exit, dropping the runtime and capture task.
        Ok(())
    } else {
        // No preview window; just run the capture loop until process exit or error.
        capture_handle.await??;
        Ok(())
    }
}

