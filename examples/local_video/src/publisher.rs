use anyhow::Result;
use clap::Parser;
use livekit::webrtc::native::yuv_helper;
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{error, info, warn};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use std::env;
use std::time::{Duration, Instant};

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
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = std::sync::Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Setup camera
    let index = CameraIndex::Index(args.camera_index as u32);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, requested)?;
    // Try to honor requested size/fps if supported
    let _ = camera.set_camera_format(CameraFormat::new(
        Resolution::new(args.width, args.height),
        FrameFormat::MJPEG,
        args.fps,
    ));
    camera.open_stream()?;
    let fmt = camera.camera_format();
    let width = fmt.width();
    let height = fmt.height();
    let fps = fmt.frame_rate();
    info!("Camera opened: {}x{} @ {} fps", width, height, fps);

    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height });
    let track = LocalVideoTrack::create_video_track(
        "camera",
        RtcVideoSource::Native(rtc_source.clone()),
    );

    room
        .local_participant()
        .publish_track(
            LocalTrack::Video(track.clone()),
            TrackPublishOptions {
                source: TrackSource::Camera,
                simulcast: true,
                video_codec: VideoCodec::H264,
                ..Default::default()
            },
        )
        .await?;
    info!("Published camera track");

    // Reusable I420 buffer and frame
    let mut frame = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: 0, buffer: I420Buffer::new(width, height) };

    // Capture loop
    let mut last = Instant::now();
    loop {
        // Get frame as RGB
        let frame_buf = camera.frame()?;
        let rgb = frame_buf.decode_image::<RgbFormat>()?;
        let rgba_stride = (width * 4) as u32;

        // Convert RGB to ABGR in-place buffer (expand to 4 channels)
        // Build a temporary ABGR buffer
        let mut abgr = vec![0u8; (width * height * 4) as usize];
        for (i, chunk) in rgb.as_raw().chunks_exact(3).enumerate() {
            let r = chunk[0];
            let g = chunk[1];
            let b = chunk[2];
            let o = i * 4;
            // ABGR layout
            abgr[o] = 255;
            abgr[o + 1] = b;
            abgr[o + 2] = g;
            abgr[o + 3] = r;
        }

        // Fill i420 buffer
        let (stride_y, stride_u, stride_v) = frame.buffer.strides();
        let (data_y, data_u, data_v) = frame.buffer.data_mut();
        yuv_helper::abgr_to_i420(
            &abgr,
            rgba_stride,
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            width as i32,
            height as i32,
        );

        rtc_source.capture_frame(&frame);

        // Simple pacing
        let elapsed = last.elapsed();
        let target = Duration::from_secs_f32(1.0 / fps as f32);
        if elapsed < target { tokio::time::sleep(target - elapsed).await; }
        last = Instant::now();
    }
}


