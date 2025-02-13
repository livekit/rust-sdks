use livekit::prelude::*;
use livekit_api::access_token;
use std::env;
use image::GenericImageView;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use livekit::options::TrackPublishOptions;
// use livekit_ffi::livekit::proto::track::VideoCodec;
use livekit::options::VideoCodec;
use tokio::signal;
use livekit::webrtc::video_source::RtcVideoSource;
use livekit::webrtc::video_source::VideoResolution;
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::native::NativeVideoSource,
};
use tokio::sync::Notify;
use std::time::{Instant};

// Connect to a room using the specified env variables
// and print all incoming events
// const WIDTH: usize = 1280;
// const HEIGHT: usize = 720;

const WIDTH: usize = 1440;
const HEIGHT: usize = 1080;

#[tokio::main]
async fn main() {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "dobby".to_string(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap();

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    log::info!("Connected to room: {} - {}", room.name(), String::from(room.sid().await));

    // Create a video source and track
    let source = NativeVideoSource::new(VideoResolution {
        width: WIDTH as u32,
        height: HEIGHT as u32,
    });
    let track = LocalVideoTrack::create_video_track(
        "image",
        RtcVideoSource::Native(source.clone()),
    );

    // Publish the track
    let options = TrackPublishOptions {
        source: TrackSource::Camera,
        video_codec: VideoCodec::H264,
        ..Default::default()
    };
    let publication = room.local_participant().publish_track(LocalTrack::Video(track.clone()), options).await.unwrap();
    println!("Published track with SID: {}", publication.sid());

    // Start displaying the image
    // tokio::spawn(display_image(source));
    // Create a Notify object to signal termination
    let notify = Arc::new(Notify::new());
    let notify_clone = notify.clone();
    tokio::spawn(async move {
        display_image(source, notify_clone).await;
    });

    // Wait for termination signals
    signal::ctrl_c().await.unwrap();
    // room.disconnect().await?;
    // Ok(())
}

async fn display_image(video_source: NativeVideoSource, notify: Arc<Notify>) {
    let frame_duration = Duration::from_millis(33); // Approx. 30 FPS

    // Load the image
    let img1 = image::open("/home/integration/test_image1.png").expect("Failed to open image");
    let img1 = img1.resize_exact(WIDTH as u32, HEIGHT as u32, image::imageops::FilterType::Nearest);
    let img2 = image::open("/home/integration/test_image2.png").expect("Failed to open image");
    let img2 = img2.resize_exact(WIDTH as u32, HEIGHT as u32, image::imageops::FilterType::Nearest);

    let mut argb_frame = vec![0u8; WIDTH * HEIGHT * 4];
    let mut y_plane = vec![0u8; WIDTH * HEIGHT];
    let mut u_plane = vec![0u8; WIDTH * HEIGHT / 4];
    let mut v_plane = vec![0u8; WIDTH * HEIGHT / 4];

    let mut last_switch = Instant::now();

    let mut current_img = &img1;

    loop {
        let start_time = tokio::time::Instant::now();

        tokio::select! {
            _ = notify.notified() => {
                log::info!("Shutting down display_image loop");
                break;
            }
            _ = async {

        // Check if 5 seconds have passed
        if last_switch.elapsed() >= Duration::from_secs(5) {
            // Switch the image
            if current_img == &img1 {
                current_img = &img2;
            } else {
                current_img = &img1;
            }
            // Reset the timer
            last_switch = Instant::now();
        }

        // Fill the frame buffer with the image data
        for (x, y, pixel) in current_img.pixels() {
            let i = (y as usize * WIDTH + x as usize) * 4;
            argb_frame[i] = pixel[2]; // B
            argb_frame[i + 1] = pixel[1]; // G
            argb_frame[i + 2] = pixel[0]; // R
            argb_frame[i + 3] = 255; // A
        }

        let mut video_frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            buffer: I420Buffer::new(WIDTH as u32, HEIGHT as u32),
            timestamp_us: 0,
        };

        let i420_buffer = &mut video_frame.buffer;

        let (stride_y, stride_u, stride_v) = i420_buffer.strides();
        let (data_y, data_u, data_v) = i420_buffer.data_mut();

        // Convert ARGB to I420 using abgr_to_i420
        yuv_helper::abgr_to_i420(
            &argb_frame,
            (WIDTH * 4) as u32,
            data_y,
            stride_y,
            data_u,
            stride_u,
            data_v,
            stride_v,
            WIDTH as i32,
            HEIGHT as i32,
        );

        video_source.capture_frame(&video_frame);
    } => {},
}


        // Sleep to maintain the frame rate
        let elapsed = start_time.elapsed();
        if frame_duration > elapsed {
            sleep(frame_duration - elapsed).await;
        }
    }
}