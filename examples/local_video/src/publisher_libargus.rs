mod publisher_common;

use anyhow::Result;
use clap::Parser;
use publisher_common::PublishCommonArgs;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available LibArgus cameras and exit
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

    /// Number of reusable DMA-BUF frames to keep in the LibArgus pool
    #[arg(long, default_value_t = 4)]
    dmabuf_pool_size: u32,

    #[command(flatten)]
    publish: PublishCommonArgs,
}

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
fn main() -> Result<()> {
    env_logger::init();
    let _ = Args::parse();
    eprintln!("publisher_libargus only runs on Linux aarch64 Jetson devices");
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use livekit::webrtc::video_frame::native::NativeBuffer;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use livekit::webrtc::video_frame::{VideoFrame, VideoRotation};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use livekit::webrtc::video_source::native::NativeVideoSource;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use livekit::webrtc::video_source::VideoResolution;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use log::info;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use nokhwa::{
    backends::capture::{query_libargus, LibArgusCamera},
    utils::{
        CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution,
    },
};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use publisher_common::{
    connect_and_publish_video_with_source, timestamp_us_from_duration, PublishedVideoContext,
};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use std::sync::{Arc, OnceLock};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use std::time::Instant;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn list_cameras() -> Result<()> {
    let cameras = query_libargus()?;
    println!("Available LibArgus cameras:");
    for camera in cameras {
        println!("{}: {}", camera.index(), camera.human_name());
    }
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    let ctrl_c_received = Arc::new(AtomicBool::new(false));
    run(args, ctrl_c_received).await
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    if args.list_cameras {
        return list_cameras();
    }

    let camera = query_libargus()?
        .into_iter()
        .find(|camera| camera.index() == &CameraIndex::Index(args.camera_index as u32))
        .ok_or_else(|| {
            anyhow::anyhow!("No LibArgus camera found for index {}", args.camera_index)
        })?;

    let target =
        CameraFormat::new(Resolution::new(args.width, args.height), FrameFormat::NV12, args.fps);
    let request =
        RequestedFormat::with_formats(RequestedFormatType::Closest(target), &[FrameFormat::NV12]);

    let mut libargus = LibArgusCamera::open(camera.index(), request)?;
    libargus.configure_dmabuf_pool(args.dmabuf_pool_size)?;
    libargus.start()?;

    let format = libargus.camera_format();
    let width = format.width();
    let height = format.height();
    let fps = format.frame_rate();
    info!(
        "LibArgus opened: {}x{} @ {} fps (camera: {})",
        width,
        height,
        fps,
        libargus.camera_info().human_name()
    );

    // Suppress the NativeVideoSource warm-up loop before any .await.
    // The warm-up sends black I420 frames that the Jetson MMAPI encoder
    // (DMABUF-only input) cannot encode, causing WebRTC to fall back to a
    // software encoder that rejects NVMM native buffers.
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    rtc_source.skip_warmup();
    let source_resolution = rtc_source.video_resolution();
    info!(
        "NativeVideoSource configured: {}x{} (camera output {}x{})",
        source_resolution.width, source_resolution.height, width, height
    );
    if publish_debug_enabled() {
        info!(
            "[publisher] LK_PUBLISH_DEBUG enabled: source={}x{}, camera={}x{}, fps={}, dmabuf_pool_size={}",
            source_resolution.width,
            source_resolution.height,
            width,
            height,
            fps,
            args.dmabuf_pool_size
        );
    }

    let PublishedVideoContext { room: _room, rtc_source } = connect_and_publish_video_with_source(
        &args.publish,
        "libargus-camera",
        fps as f64,
        rtc_source,
    )
    .await?;

    // Run the blocking capture loop on a dedicated OS thread so that the
    // async runtime stays responsive to the Ctrl-C signal.  LibArgusCamera
    // is !Send, so it must live entirely on this thread.
    let ctrl_c_flag = Arc::clone(&ctrl_c_received);
    let capture_thread = std::thread::spawn(move || -> Result<()> {
        let mut frames: u64 = 0;
        let mut total_frames: u64 = 0;
        let mut last_log = Instant::now();
        let mut last_debug_log = Instant::now();
        let mut last_timestamp_us: Option<i64> = None;

        while !ctrl_c_received.load(Ordering::Acquire) {
            let frame = match libargus.frame_dmabuf_pooled() {
                Ok(f) => f,
                Err(e) => {
                    if ctrl_c_received.load(Ordering::Acquire) {
                        break;
                    }
                    return Err(e.into());
                }
            };

            let resolution = frame.resolution();
            let capture_timestamp = frame.capture_timestamp();
            let timestamp_us = timestamp_us_from_duration(frame.capture_timestamp());
            let dmabuf_fd = frame.dmabuf_fd();
            let bytes_used = frame.bytes_used();
            let y_stride = frame.y_stride();
            let uv_stride = frame.uv_stride();
            total_frames += 1;
            let should_log_debug = publish_debug_enabled()
                && (total_frames <= 10 || last_debug_log.elapsed().as_secs_f64() >= 2.0);

            if should_log_debug {
                let timestamp_delta =
                    last_timestamp_us.map(|prev| timestamp_us.saturating_sub(prev));
                info!(
                    "[publisher] LibArgus frame #{}: fd={}, {}x{}, y_stride={}, uv_stride={}, bytes_used={}, capture_ts={:?}, timestamp_us={}, delta_us={:?}",
                    total_frames,
                    dmabuf_fd,
                    resolution.width(),
                    resolution.height(),
                    y_stride,
                    uv_stride,
                    bytes_used,
                    capture_timestamp,
                    timestamp_us,
                    timestamp_delta
                );
                last_debug_log = Instant::now();
            }
            last_timestamp_us = Some(timestamp_us);

            let video_frame: VideoFrame<NativeBuffer> = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us,
                buffer: unsafe {
                    NativeBuffer::from_jetson_dmabuf(
                        dmabuf_fd,
                        resolution.width(),
                        resolution.height(),
                        y_stride,
                        uv_stride,
                        Some(Box::new(frame)),
                    )
                },
            };

            if should_log_debug {
                info!(
                    "[publisher] VideoFrame #{} built: fd={}, {}x{}, timestamp_us={}, submitting to NativeVideoSource",
                    total_frames,
                    dmabuf_fd,
                    resolution.width(),
                    resolution.height(),
                    timestamp_us
                );
            }
            rtc_source.capture_frame(&video_frame);
            if should_log_debug {
                info!(
                    "[publisher] VideoFrame #{} submitted to NativeVideoSource",
                    total_frames
                );
                last_debug_log = Instant::now();
            }
            frames += 1;

            if last_log.elapsed().as_secs_f64() >= 2.0 {
                let elapsed = last_log.elapsed().as_secs_f64();
                info!(
                    "Publishing LibArgus video: {}x{}, ~{:.1} fps",
                    width,
                    height,
                    frames as f64 / elapsed,
                );
                frames = 0;
                last_log = Instant::now();
            }
        }

        info!("Stopping LibArgus camera...");
        libargus.stop()?;
        Ok(())
    });

    // Wait for Ctrl-C on the async side, signal the capture thread, then join it.
    let _ = tokio::signal::ctrl_c().await;
    ctrl_c_flag.store(true, Ordering::Release);
    info!("Ctrl-C received, shutting down...");

    capture_thread.join().map_err(|e| anyhow::anyhow!("capture thread panicked: {:?}", e))??;
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn publish_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("LK_PUBLISH_DEBUG").is_ok())
}
