mod publisher_common;

use anyhow::Result;
use clap::Parser;
use publisher_common::{spawn_ctrl_c_handler, PublishCommonArgs};

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
    connect_and_publish_video, timestamp_us_from_duration, PublishedVideoContext,
};
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use std::sync::atomic::Ordering;
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
    run(args, spawn_ctrl_c_handler()).await
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
async fn run(
    args: Args,
    ctrl_c_received: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
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

    let PublishedVideoContext { room: _room, rtc_source } = connect_and_publish_video(
        &args.publish,
        "libargus-camera",
        VideoResolution { width, height },
        fps as f64,
    )
    .await?;

    let mut frames: u64 = 0;
    let mut last_log = Instant::now();

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let frame = libargus.frame_dmabuf_pooled()?;
        let resolution = frame.resolution();
        let timestamp_us = timestamp_us_from_duration(frame.capture_timestamp());
        let dmabuf_fd = frame.dmabuf_fd();
        let y_stride = frame.y_stride();
        let uv_stride = frame.uv_stride();

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

        rtc_source.capture_frame(&video_frame);
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

    libargus.stop()?;
    Ok(())
}
