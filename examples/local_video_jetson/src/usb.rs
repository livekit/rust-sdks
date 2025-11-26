use anyhow::Result;
use clap::Parser;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use gstreamer::prelude::*;
use log::{info, warn};

mod common;
use common::{connect_and_publish, BaseArgs, CpuNv12Pusher};
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use common::push_dmabuf_nv12;

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
    let (_room, rtc_source, _track) = connect_and_publish(&args.base, width, height).await?;

    // Build pipelines in order of preference and fallback if the device does not support them.
    // Many USB webcams output MJPG or YUY2; handle both robustly before failing.
    let mut active_pipeline: Option<gst::Pipeline> = None;
    let mut active_sink: Option<gst_app::AppSink> = None;
    let mut info: Option<gst_video::VideoInfo> = None;
    let mut used_dmabuf = false;

    // Candidate 1: dmabuf NV12 (zero-copy), only when compiled with dmabuf feature on Linux.
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    {
        let pipeline_str = format!(
            "v4l2src device={} io-mode=dmabuf ! \
             video/x-raw(memory:DMABuf),format=NV12,width={},height={},framerate={}/1 ! \
             identity name=ident ! \
             appsink name=sink max-buffers=2 drop=true sync=false",
            args.device, width, height, fps
        );
        if let Ok((pipeline, sink)) = build_pipeline(&pipeline_str) {
            if let Some(sample) = try_start_and_sample(&pipeline, &sink, 5) {
                if let Some(caps) = sample.caps() {
                    if let Ok(vi) = gst_video::VideoInfo::from_caps(caps) {
                        info!("Using DMABUF NV12 pipeline");
                        used_dmabuf = true;
                        active_pipeline = Some(pipeline);
                        active_sink = Some(sink);
                        info = Some(vi);
                    }
                }
            }
        }
    }

    // Candidate 2: system-memory raw -> NV12 via videoconvert.
    if active_pipeline.is_none() {
        let pipeline_str = format!(
            "v4l2src device={} ! \
             videoconvert ! video/x-raw,format=NV12,width={},height={},framerate={}/1 ! \
             appsink name=sink max-buffers=2 drop=true sync=false",
            args.device, width, height, fps
        );
        if let Ok((pipeline, sink)) = build_pipeline(&pipeline_str) {
            if let Some(sample) = try_start_and_sample(&pipeline, &sink, 5) {
                if let Some(caps) = sample.caps() {
                    if let Ok(vi) = gst_video::VideoInfo::from_caps(caps) {
                        info!("Using system-memory NV12 (videoconvert) pipeline");
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

    // Candidate 3: explicit YUYV (YUY2) -> NV12 via videoconvert. Many cameras expose YUYV.
    if active_pipeline.is_none() {
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

    // Candidate 4: MJPG -> jpegdec -> NV12
    if active_pipeline.is_none() {
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

    let mut pipeline = match active_pipeline {
        Some(p) => p,
        None => {
            return Err(anyhow::anyhow!(
                "No working pipeline produced frames (tried dmabuf, raw+convert, and MJPG)"
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
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    let mut cpu_pusher_opt: Option<CpuNv12Pusher> = None;
    #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
    let mut cpu_pusher_opt: Option<CpuNv12Pusher> = None;
    {
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        #[cfg(all(feature = "dmabuf", target_os = "linux"))]
        if used_dmabuf {
            unsafe {
                let _ = push_dmabuf_nv12(&rtc_source, &info, buffer);
            }
        } else {
            let strides = info.stride();
            let stride_y = strides[0] as u32;
            let stride_uv = strides[1] as u32;
            let mut p = CpuNv12Pusher::new(width, height, stride_y, stride_uv);
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            p.push(&rtc_source, y, uv);
            cpu_pusher_opt = Some(p);
        }
        #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
        {
            let strides = info.stride();
            let stride_y = strides[0] as u32;
            let stride_uv = strides[1] as u32;
            let mut p = CpuNv12Pusher::new(width, height, stride_y, stride_uv);
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            p.push(&rtc_source, y, uv);
            cpu_pusher_opt = Some(p);
        }
    }

    info!("USB V4L2 capture started: {} ({}) {}x{} @ {} fps", args.device, "NV12", width, height, fps);

    // Main loop
    loop {
        let sample = match sink.try_pull_sample(gst::ClockTime::from_seconds(2)) {
            Some(s) => s,
            None => break,
        };
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        #[cfg(all(feature = "dmabuf", target_os = "linux"))]
        if used_dmabuf {
            unsafe {
                let _ = push_dmabuf_nv12(&rtc_source, &info, buffer);
            }
        } else {
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            if let Some(ref mut pusher) = cpu_pusher_opt {
                pusher.push(&rtc_source, y, uv);
            }
        }
        #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
        {
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            if let Some(ref mut pusher) = cpu_pusher_opt {
                pusher.push(&rtc_source, y, uv);
            }
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}


