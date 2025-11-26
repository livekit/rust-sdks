use anyhow::Result;
use clap::{Parser, ValueEnum};
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer::prelude::*;
use log::{debug, info, warn};
use std::time::{Duration, Instant};

mod common;
use common::{connect_and_publish, BaseArgs, CpuNv12Pusher};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CaptureFormat {
    Nv12,
    /// V4L2 fourcc 'YUYV' (GStreamer caps name 'YUY2')
    #[value(alias = "yuy2")]
    Yuyv,
    Mjpg,
}

fn luma_stats(y: &[u8]) -> (u8, u8, f32) {
    if y.is_empty() {
        return (0, 0, 0.0);
    }
    let mut min_v = u8::MAX;
    let mut max_v = u8::MIN;
    let mut sum: u64 = 0;
    for &v in y {
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
        sum += v as u64;
    }
    let mean = sum as f32 / y.len() as f32;
    (min_v, max_v, mean)
}

fn build_pipeline(
    pipeline_str: &str,
) -> anyhow::Result<(gst::Pipeline, gst_app::AppSink)> {
    let pipeline = gst::parse_launch(pipeline_str)?
        .downcast::<gst::Pipeline>()
        .expect("pipeline");
    let sink = pipeline
        .by_name("sink")
        .and_then(|e| e.downcast::<gst_app::AppSink>().ok())
        .expect("appsink");
    Ok((pipeline, sink))
}

fn try_start_and_sample(
    pipeline: &gst::Pipeline,
    sink: &gst_app::AppSink,
    timeout_secs: u64,
) -> Option<gst::Sample> {
    if pipeline.set_state(gst::State::Playing).is_err() {
        return None;
    }
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(timeout_secs));
    if sample.is_none() {
        let _ = pipeline.set_state(gst::State::Null);
    }
    sample
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    base: BaseArgs,
    /// V4L2 device path
    #[arg(long, default_value = "/dev/video0")]
    device: String,
    /// Capture format to request from the camera (nv12, yuyv, mjpg)
    #[arg(long, value_enum, default_value = "mjpg")]
    format: CaptureFormat,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    gst::init()?;

    let args = Args::parse();
    let width = args.base.width;
    let height = args.base.height;
    let fps = args.base.fps;

    // Connect and publish
    let (_room, rtc_source, track) = connect_and_publish(&args.base, width, height).await?;

    // Periodically log outbound stats (encoder, resolution, fps, bitrate)
    tokio::spawn({
        let track = track.clone();
        async move {
            loop {
                if let Ok(stats) = track.get_stats().await {
                    for s in stats {
                        if let livekit::webrtc::stats::RtcStats::OutboundRtp(o) = s {
                            info!(
                                "Outbound video stats: {}x{} ~{:.1} fps, target_bitrate={:.1} kbps, encoder='{}', active={}",
                                o.outbound.frame_width,
                                o.outbound.frame_height,
                                o.outbound.frames_per_second,
                                o.outbound.target_bitrate / 1000.0,
                                o.outbound.encoder_implementation,
                                o.outbound.active,
                            );
                        }
                    }
                } else {
                    debug!("Failed to fetch outbound video stats");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });

    // Build pipeline for the explicitly requested format.
    let mut active_pipeline: Option<gst::Pipeline> = None;
    let mut active_sink: Option<gst_app::AppSink> = None;
    let mut info: Option<gst_video::VideoInfo> = None;

    match args.format {
        CaptureFormat::Nv12 => {
            // Request NV12 directly from the camera.
            let pipeline_str = format!(
                "v4l2src device={} ! \
                 video/x-raw,format=NV12,width={},height={},framerate={}/1 ! \
                 appsink name=sink max-buffers=2 drop=true sync=false",
                args.device, width, height, fps
            );
            if let Ok((pipeline, sink)) = build_pipeline(&pipeline_str) {
                if let Some(sample) = try_start_and_sample(&pipeline, &sink, 5) {
                    if let Some(caps) = sample.caps() {
                        if let Ok(vi) = gst_video::VideoInfo::from_caps(caps) {
                            info!("Using NV12 pipeline");
                            active_pipeline = Some(pipeline);
                            active_sink = Some(sink);
                            info = Some(vi);
                        }
                    }
                } else {
                    let _ = pipeline.set_state(gst::State::Null);
                }
            }
        }
        CaptureFormat::Yuyv => {
            // Explicit YUYV (YUY2) -> NV12 via videoconvert.
            let pipeline_str = format!(
                "v4l2src device={} ! \
                 video/x-raw,format=YUY2,width={},height={},framerate={}/1 ! \
                 videoconvert ! video/x-raw,format=NV12 ! \
                 appsink name=sink max-buffers=2 drop=true sync=false",
                args.device, width, height, fps
            );
            if let Ok((pipeline, sink)) = build_pipeline(&pipeline_str) {
                if let Some(sample) = try_start_and_sample(&pipeline, &sink, 5) {
                    if let Some(caps) = sample.caps() {
                        if let Ok(vi) = gst_video::VideoInfo::from_caps(caps) {
                            info!("Using YUYV (YUY2) decode pipeline");
                            active_pipeline = Some(pipeline);
                            active_sink = Some(sink);
                            info = Some(vi);
                        }
                    }
                } else {
                    let _ = pipeline.set_state(gst::State::Null);
                }
            }
        }
        CaptureFormat::Mjpg => {
            // MJPG -> jpegdec -> NV12
            let pipeline_str = format!(
                "v4l2src device={} ! \
                 image/jpeg,width={},height={},framerate={}/1 ! \
                 jpegdec ! videoconvert ! video/x-raw,format=NV12 ! \
                 appsink name=sink max-buffers=2 drop=true sync=false",
                args.device, width, height, fps
            );
            if let Ok((pipeline, sink)) = build_pipeline(&pipeline_str) {
                if let Some(sample) = try_start_and_sample(&pipeline, &sink, 5) {
                    if let Some(caps) = sample.caps() {
                        if let Ok(vi) = gst_video::VideoInfo::from_caps(caps) {
                            info!("Using MJPG decode pipeline");
                            active_pipeline = Some(pipeline);
                            active_sink = Some(sink);
                            info = Some(vi);
                        }
                    }
                } else {
                    let _ = pipeline.set_state(gst::State::Null);
                }
            }
        }
    }

    let mut pipeline = match active_pipeline {
        Some(p) => p,
        None => {
            return Err(anyhow::anyhow!(
                "Failed to negotiate requested format {:?} at {}x{} @ {} fps",
                args.format,
                width,
                height,
                fps
            ))
        }
    };
    let sink = active_sink.expect("appsink exists");
    let info = info.expect("VideoInfo available");

    // Log negotiated info derived from caps
    info!(
        "Negotiated video info: format={:?}, width={} height={}",
        info.format(),
        info.width(),
        info.height()
    );

    // We already pulled a sample to get caps for the chosen pipeline, but we didn't keep it.
    // Pull one more immediately to initialize the pusher path.
    let sample = match sink.try_pull_sample(gst::ClockTime::from_seconds(2)) {
        Some(s) => s,
        None => return Err(anyhow::anyhow!("pipeline EOS or timeout")),
    };
    if info.format() != gst_video::VideoFormat::Nv12 {
        warn!(
            "Negotiated format is {:?}, converting as NV12 copy",
            info.format()
        );
    }
    // Create CPU NV12 pusher and process the first sample to initialize buffer layout.
    let strides = info.stride();
    let stride_y = strides[0] as u32;
    let stride_uv = strides[1] as u32;
    let mut pusher = {
        let mut p = CpuNv12Pusher::new(width, height, stride_y, stride_uv);
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
            .map_err(|_| anyhow::anyhow!("map frame readable"))?;
        let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
        let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
        let (min_y, max_y, mean_y) = luma_stats(y);
        info!(
            "First frame Y stats: min={}, max={}, mean={:.1} ({} bytes)",
            min_y,
            max_y,
            mean_y,
            y.len()
        );
        p.push(&rtc_source, y, uv);
        p
    };

    info!("USB V4L2 capture started: {} ({}) {}x{} @ {} fps", args.device, "NV12", width, height, fps);

    // Main loop
    let mut frames: u64 = 0;
    let mut last_log = Instant::now();
    let mut last_luma_log = Instant::now();
    loop {
        let sample = match sink.try_pull_sample(gst::ClockTime::from_seconds(2)) {
            Some(s) => s,
            None => break,
        };
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
            .map_err(|_| anyhow::anyhow!("map frame readable"))?;
        let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
        let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
        pusher.push(&rtc_source, y, uv);

        frames += 1;
        if last_log.elapsed().as_secs() >= 2 {
            let fps = frames as f64 / last_log.elapsed().as_secs_f64().max(0.001);
            info!("USB capture pushing ~{:.1} fps ({} frames total)", fps, frames);
            last_log = Instant::now();
            frames = 0;
        }

        if last_luma_log.elapsed().as_secs() >= 5 {
            let (min_y, max_y, mean_y) = luma_stats(y);
            info!(
                "Sampled frame Y stats: min={}, max={}, mean={:.1}",
                min_y,
                max_y,
                mean_y
            );
            last_luma_log = Instant::now();
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}


