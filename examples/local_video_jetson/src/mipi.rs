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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    base: BaseArgs,
    /// Sensor ID (nvarguscamerasrc sensor-id)
    #[arg(long, default_value_t = 0)]
    sensor_id: i32,
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

    // Build pipeline.
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    let pipeline_str = format!(
        "nvarguscamerasrc sensor-id={} ! \
         video/x-raw(memory:NVMM),format=NV12,width={},height={},framerate={}/1 ! \
         nvvidconv ! video/x-raw(memory:DMABuf),format=NV12,width={},height={} ! \
         appsink name=sink max-buffers=2 drop=true sync=false",
        args.sensor_id, width, height, fps, width, height
    );
    #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
    let pipeline_str = format!(
        "nvarguscamerasrc sensor-id={} ! \
         video/x-raw(memory:NVMM),format=NV12,width={},height={},framerate={}/1 ! \
         nvvidconv ! video/x-raw,format=NV12,width={},height={} ! \
         appsink name=sink max-buffers=2 drop=true sync=false",
        args.sensor_id, width, height, fps, width, height
    );
    let pipeline = gst::parse_launch(&pipeline_str)?
        .downcast::<gst::Pipeline>()
        .expect("pipeline");
    let sink = pipeline
        .by_name("sink")
        .and_then(|e| e.downcast::<gst_app::AppSink>().ok())
        .expect("appsink");

    pipeline.set_state(gst::State::Playing)?;

    // Determine strides from negotiated caps on first sample
    let sample = match sink.try_pull_sample(gst::ClockTime::from_seconds(2)) {
        Some(s) => s,
        None => return Err(anyhow::anyhow!("pipeline EOS or timeout")),
    };
    let caps = sample.caps().ok_or_else(|| anyhow::anyhow!("no caps"))?;
    let info = gst_video::VideoInfo::from_caps(caps)?;
    if info.format() != gst_video::VideoFormat::Nv12 {
        warn!("Negotiated format is {:?}, converting as NV12 copy", info.format());
    }
    #[cfg(all(feature = "dmabuf", target_os = "linux"))]
    unsafe {
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        let _ = push_dmabuf_nv12(&rtc_source, &info, buffer);
    }
    #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
    let mut pusher = {
        let strides = info.stride();
        let stride_y = strides[0] as u32;
        let stride_uv = strides[1] as u32;
        let mut p = CpuNv12Pusher::new(width, height, stride_y, stride_uv);
        // process first sample
        {
            let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            p.push(&rtc_source, y, uv);
        }
        p
    };

    info!("MIPI capture started: {}x{} @ {} fps", width, height, fps);

    // Main loop
    loop {
        let sample = match sink.try_pull_sample(gst::ClockTime::from_seconds(2)) {
            Some(s) => s,
            None => break,
        };
        let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("no buffer"))?;
        #[cfg(all(feature = "dmabuf", target_os = "linux"))]
        unsafe {
            let _ = push_dmabuf_nv12(&rtc_source, &info, buffer);
        }
        #[cfg(not(all(feature = "dmabuf", target_os = "linux")))]
        {
            let vframe = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
                .map_err(|_| anyhow::anyhow!("map frame readable"))?;
            let y = vframe.plane_data(0).map_err(|_| anyhow::anyhow!("no Y plane"))?;
            let uv = vframe.plane_data(1).map_err(|_| anyhow::anyhow!("no UV plane"))?;
            pusher.push(&rtc_source, y, uv);
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}


