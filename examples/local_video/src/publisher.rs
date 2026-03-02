use anyhow::Result;
use clap::Parser;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
#[cfg(target_os = "linux")]
use livekit::webrtc::video_frame::NV12Buffer;
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
use std::time::{Duration, Instant};
use yuv_sys;

#[cfg(target_os = "linux")]
use v4l::buffer::Type as BufType;
#[cfg(target_os = "linux")]
use v4l::io::traits::CaptureStream;
#[cfg(target_os = "linux")]
use v4l::prelude::*;
#[cfg(target_os = "linux")]
use v4l::video::Capture;
#[cfg(target_os = "linux")]
use v4l::FourCC;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// Camera index to use (numeric, for nokhwa backend)
    #[arg(long, default_value_t = 0)]
    camera_index: usize,

    /// V4L2 device path (Linux only, e.g. /dev/video-camera0).
    /// When set, uses direct V4L2 capture instead of nokhwa.
    #[arg(long)]
    device: Option<String>,

    /// Pixel format to request: nv12, yuyv, or mjpeg.
    /// NV12 requires --device (V4L2 direct capture).
    #[arg(long, default_value = "nv12")]
    format: String,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PixFmt {
    Nv12,
    Yuyv,
    Mjpeg,
}

impl PixFmt {
    fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "nv12" => Ok(PixFmt::Nv12),
            "yuyv" => Ok(PixFmt::Yuyv),
            "mjpeg" | "mjpg" => Ok(PixFmt::Mjpeg),
            _ => Err(anyhow::anyhow!("Unknown pixel format '{}'. Use nv12, yuyv, or mjpeg.", s)),
        }
    }
}

fn list_cameras() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        println!("V4L2 devices:");
        for dev in v4l::context::enum_devices() {
            println!(
                "  {} - {}",
                dev.path().display(),
                dev.name().unwrap_or_default()
            );
        }
    }

    let cams = nokhwa::query(ApiBackend::Auto)?;
    println!("Nokhwa cameras:");
    for (i, cam) in cams.iter().enumerate() {
        println!("  {}. {}", i, cam.human_name());
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

async fn connect_and_publish(
    args: &Args,
    rtc_source: &NativeVideoSource,
) -> Result<Arc<Room>> {
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
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

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

    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));

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

    Ok(room)
}

async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    if args.list_cameras {
        return list_cameras();
    }

    let pix_fmt = PixFmt::parse(&args.format)?;

    // Use V4L2 direct path on Linux when --device is given or NV12 is requested
    #[cfg(target_os = "linux")]
    if args.device.is_some() || pix_fmt == PixFmt::Nv12 {
        return run_v4l2(args, pix_fmt, ctrl_c_received).await;
    }

    #[cfg(not(target_os = "linux"))]
    if pix_fmt == PixFmt::Nv12 {
        return Err(anyhow::anyhow!(
            "NV12 format requires Linux V4L2 direct capture (--device)"
        ));
    }

    run_nokhwa(args, ctrl_c_received).await
}

// ---------------------------------------------------------------------------
// V4L2 direct capture (Linux only) — supports NV12, YUYV, MJPEG
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
async fn run_v4l2(
    args: Args,
    pix_fmt: PixFmt,
    ctrl_c_received: Arc<AtomicBool>,
) -> Result<()> {
    let dev_path = args.device.clone().unwrap_or_else(|| "/dev/video-camera0".to_string());
    info!("Opening V4L2 device: {}", dev_path);

    let dev = Device::with_path(&dev_path)?;

    let format_descs: Vec<_> = dev.enum_formats()?.collect();
    info!("Device supports {} format(s):", format_descs.len());
    for fd in &format_descs {
        info!("  {:?}", fd);
    }

    let fourcc = match pix_fmt {
        PixFmt::Nv12 => FourCC::new(b"NV12"),
        PixFmt::Yuyv => FourCC::new(b"YUYV"),
        PixFmt::Mjpeg => FourCC::new(b"MJPG"),
    };

    let mut fmt = dev.format()?;
    fmt.width = args.width;
    fmt.height = args.height;
    fmt.fourcc = fourcc;
    let fmt = dev.set_format(&fmt)?;

    let width = fmt.width;
    let height = fmt.height;
    info!(
        "V4L2 negotiated: {}x{} fourcc={} stride={}",
        width, height, fmt.fourcc, fmt.stride
    );

    let params = v4l::video::capture::Parameters::with_fps(args.fps);
    let params = dev.set_params(&params)?;
    info!(
        "V4L2 framerate: {}/{}",
        params.interval.denominator, params.interval.numerator
    );

    let rtc_source = NativeVideoSource::new(VideoResolution { width, height });
    let _room = connect_and_publish(&args, &rtc_source).await?;

    let pace_fps = args.fps as f64;
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / pace_fps));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    let start_ts = Instant::now();

    let mut stream = v4l::io::mmap::Stream::with_buffers(&dev, BufType::VideoCapture, 4)?;

    let target = Duration::from_secs_f64(1.0 / pace_fps);
    info!(
        "V4L2 capture started: {}x{} {:?}, target {:.2} ms",
        width, height, pix_fmt,
        target.as_secs_f64() * 1000.0
    );

    if pix_fmt == PixFmt::Nv12 {
        info!("NV12 zero-conversion path: camera -> NV12Buffer -> MPP encoder");
        run_v4l2_nv12_loop(
            &rtc_source, &mut stream, &fmt, width, height,
            pace_fps, target, start_ts, &ctrl_c_received,
        )?;
    } else {
        run_v4l2_convert_loop(
            &rtc_source, &mut stream, pix_fmt, width, height,
            pace_fps, target, start_ts, &ctrl_c_received,
        )?;
    }

    Ok(())
}

/// NV12 fast path: copy camera NV12 data directly into an NV12Buffer.
/// No pixel format conversion — the MPP encoder accepts NV12 natively.
#[cfg(target_os = "linux")]
fn run_v4l2_nv12_loop(
    rtc_source: &NativeVideoSource,
    stream: &mut v4l::io::mmap::Stream,
    fmt: &v4l::Format,
    width: u32,
    height: u32,
    _pace_fps: f64,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
) -> Result<()> {
    let src_stride_y = fmt.stride as u32;
    let src_stride_uv = fmt.stride as u32;
    let mut nv12_buf = NV12Buffer::with_strides(width, height, src_stride_y, src_stride_uv);

    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        buffer: NV12Buffer::new(width, height),
    };

    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_copy_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors: u32 = 0;
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
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    return Err(anyhow::anyhow!(
                        "V4L2 capture failed {} times: {}",
                        consecutive_errors,
                        e
                    ));
                }
                log::warn!("V4L2 error ({}/{}): {}", consecutive_errors, MAX_ERRORS, e);
                continue;
            }
        };
        let t1 = Instant::now();

        // Copy NV12 data from the mmap buffer into the NV12Buffer
        let y_plane_size = (src_stride_y as usize) * (height as usize);
        let uv_plane_size = (src_stride_uv as usize) * ((height as usize + 1) / 2);
        let (dst_y, dst_uv) = nv12_buf.data_mut();
        let copy_y = y_plane_size.min(dst_y.len()).min(buf.len());
        dst_y[..copy_y].copy_from_slice(&buf[..copy_y]);
        let uv_start = y_plane_size;
        let copy_uv = uv_plane_size.min(dst_uv.len()).min(buf.len().saturating_sub(uv_start));
        dst_uv[..copy_uv].copy_from_slice(&buf[uv_start..uv_start + copy_uv]);
        let t2 = Instant::now();

        // Swap the buffer into the frame (avoids allocation each iteration)
        std::mem::swap(&mut frame.buffer, &mut nv12_buf);
        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
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
                "{}x{} NV12 ~{:.1} fps | avg ms: v4l2 {:.2}, copy {:.2}, pub {:.2} | target {:.2}",
                width, height, fps_est,
                sum_get_ms / n, sum_copy_ms / n, sum_capture_ms / n,
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

/// YUYV/MJPEG path: convert to I420 via libyuv before publishing.
#[cfg(target_os = "linux")]
fn run_v4l2_convert_loop(
    rtc_source: &NativeVideoSource,
    stream: &mut v4l::io::mmap::Stream,
    pix_fmt: PixFmt,
    width: u32,
    height: u32,
    _pace_fps: f64,
    target: Duration,
    start_ts: Instant,
    ctrl_c_received: &AtomicBool,
) -> Result<()> {
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        buffer: I420Buffer::new(width, height),
    };

    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let mut sum_get_ms = 0.0;
    let mut sum_convert_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut consecutive_errors: u32 = 0;
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
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_ERRORS {
                    return Err(anyhow::anyhow!(
                        "V4L2 capture failed {} times: {}",
                        consecutive_errors,
                        e
                    ));
                }
                log::warn!("V4L2 error ({}/{}): {}", consecutive_errors, MAX_ERRORS, e);
                continue;
            }
        };
        let t1 = Instant::now();

        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();

        match pix_fmt {
            PixFmt::Nv12 => unreachable!("NV12 handled by nv12 loop"),
            PixFmt::Yuyv => {
                let src_stride = (width * 2) as i32;
                unsafe {
                    yuv_sys::rs_YUY2ToI420(
                        buf.as_ptr(),
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
            }
            PixFmt::Mjpeg => {
                let ret = unsafe {
                    yuv_sys::rs_MJPGToI420(
                        buf.as_ptr(),
                        buf.len(),
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
                    )
                };
                if ret != 0 {
                    log::warn!("MJPGToI420 failed (ret={}), skipping", ret);
                    continue;
                }
            }
        }
        let t2 = Instant::now();

        frame.timestamp_us = start_ts.elapsed().as_micros() as i64;
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
                "{}x{} {:?} ~{:.1} fps | avg ms: v4l2 {:.2}, cvt {:.2}, pub {:.2} | target {:.2}",
                width, height, pix_fmt, fps_est,
                sum_get_ms / n, sum_convert_ms / n, sum_capture_ms / n,
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

// ---------------------------------------------------------------------------
// nokhwa capture path (cross-platform, YUYV/MJPEG)
// ---------------------------------------------------------------------------
async fn run_nokhwa(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    let index = CameraIndex::Index(args.camera_index as u32);
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, requested)?;

    let wanted =
        CameraFormat::new(Resolution::new(args.width, args.height), FrameFormat::YUYV, args.fps);
    let mut using_fmt = "YUYV";
    if camera
        .set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(wanted)))
        .is_err()
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

    let rtc_source = NativeVideoSource::new(VideoResolution { width, height });
    let _room = connect_and_publish(&args, &rtc_source).await?;

    let pace_fps = args.fps as f64;
    let mut frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: 0,
        buffer: I420Buffer::new(width, height),
    };
    let is_yuyv = fmt.format() == FrameFormat::YUYV;
    info!(
        "Conversion: {}",
        if is_yuyv { "YUYV->I420 (libyuv)" } else { "Auto (RGB24 or MJPEG)" }
    );

    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / pace_fps));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    let start_ts = Instant::now();

    let mut frames: u64 = 0;
    let mut last_fps_log = Instant::now();
    let target = Duration::from_secs_f64(1.0 / pace_fps);
    info!("Target frame interval: {:.2} ms", target.as_secs_f64() * 1000.0);

    let mut sum_get_ms = 0.0;
    let mut sum_decode_ms = 0.0;
    let mut sum_convert_ms = 0.0;
    let mut sum_capture_ms = 0.0;
    let mut sum_sleep_ms = 0.0;
    let mut sum_iter_ms = 0.0;
    let mut logged_mjpeg_fallback = false;
    let mut consecutive_capture_errors: u32 = 0;
    const MAX_CONSECUTIVE_CAPTURE_ERRORS: u32 = 30;

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        let wait_start = Instant::now();
        ticker.tick().await;
        let iter_start = Instant::now();

        let t0 = Instant::now();
        let frame_buf = match camera.frame() {
            Ok(buf) => {
                consecutive_capture_errors = 0;
                buf
            }
            Err(e) => {
                consecutive_capture_errors += 1;
                if consecutive_capture_errors >= MAX_CONSECUTIVE_CAPTURE_ERRORS {
                    return Err(anyhow::anyhow!(
                        "Camera capture failed {} consecutive times, last error: {}",
                        consecutive_capture_errors,
                        e
                    ));
                }
                log::warn!(
                    "Frame capture error (attempt {}/{}): {} -- skipping frame",
                    consecutive_capture_errors,
                    MAX_CONSECUTIVE_CAPTURE_ERRORS,
                    e
                );
                continue;
            }
        };
        let t1 = Instant::now();
        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();

        let t2 = if is_yuyv {
            let src = frame_buf.buffer();
            let src_bytes = src.as_ref();
            let src_stride = (width * 2) as i32;
            let t2_local = t1;
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
            t2_local
        } else {
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
