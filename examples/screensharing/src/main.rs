use clap::Parser;
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit::track::{LocalTrack, LocalVideoTrack, TrackSource};
use livekit::webrtc::desktop_capturer::{
    CaptureResult, DesktopCapturer, DesktopCapturerOptions, DesktopFrame,
};
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{
    I420Buffer, RtcVideoSource, VideoBuffer, VideoFrame, VideoResolution, VideoRotation,
};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit_api::access_token;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Capture the mouse cursor
    #[arg(long)]
    capture_cursor: bool,

    /// Capture a specific window (requires window ID)
    #[arg(long)]
    capture_window: bool,

    /// Use system screen picker (macOS only)
    #[cfg(target_os = "macos")]
    #[arg(long)]
    use_system_picker: bool,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();

    #[cfg(target_os = "linux")]
    {
        /* This is needed for getting the system picker for screen sharing. */
        use glib::MainLoop;
        let main_loop = MainLoop::new(None, false);
        let _handle = std::thread::spawn(move || {
            main_loop.run();
        });
    }

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "dev_room".to_string(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap();

    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    log::info!("Connected to room: {} - {}", room.name(), String::from(room.sid().await));

    let stream_width = 1920;
    let stream_height = 1080;
    let buffer_source =
        NativeVideoSource::new(VideoResolution { width: stream_width, height: stream_height });
    let track = LocalVideoTrack::create_video_track(
        "screen_share",
        RtcVideoSource::Native(buffer_source.clone()),
    );

    room.local_participant()
        .publish_track(
            LocalTrack::Video(track),
            TrackPublishOptions {
                source: TrackSource::Screenshare,
                video_codec: VideoCodec::VP9,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let buffer_source_clone = buffer_source.clone();
    let video_frame = Arc::new(Mutex::new(VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        buffer: I420Buffer::new(stream_width, stream_height),
        timestamp_us: 0,
    }));
    let capture_buffer = Arc::new(Mutex::new(I420Buffer::new(stream_width, stream_height)));
    let callback = move |result: CaptureResult, frame: DesktopFrame| {
        match result {
            CaptureResult::ErrorTemporary => {
                log::info!("Error temporary");
                return;
            }
            CaptureResult::ErrorPermanent => {
                log::info!("Error permanent");
                return;
            }
            _ => {}
        }
        let video_frame = video_frame.clone();
        let height = frame.height();
        let width = frame.width();

        {
            let mut capture_buffer = capture_buffer.lock().unwrap();
            let capture_buffer_width = capture_buffer.width() as i32;
            let capture_buffer_height = capture_buffer.height() as i32;
            if height != capture_buffer_height || width != capture_buffer_width {
                *capture_buffer = I420Buffer::new(width as u32, height as u32);
            }
        }

        let stride = frame.stride();
        let data = frame.data();

        let mut capture_buffer = capture_buffer.lock().unwrap();
        let (s_y, s_u, s_v) = capture_buffer.strides();
        let (y, u, v) = capture_buffer.data_mut();
        yuv_helper::argb_to_i420(data, stride, y, s_y, u, s_u, v, s_v, width, height);

        let scaled_buffer = capture_buffer.scale(stream_width as i32, stream_height as i32);
        let (scaled_y, scaled_u, scaled_v) = scaled_buffer.data();

        let mut framebuffer = video_frame.lock().unwrap();
        let buffer = &mut framebuffer.buffer;
        let (y, u, v) = buffer.data_mut();
        y.copy_from_slice(scaled_y);
        u.copy_from_slice(scaled_u);
        v.copy_from_slice(scaled_v);

        buffer_source_clone.capture_frame(&*framebuffer);
    };
    let mut options = DesktopCapturerOptions::new();
    #[cfg(target_os = "macos")]
    {
        options.set_sck_system_picker(args.use_system_picker);
    }
    options.set_window_capturer(args.capture_window);
    options.set_include_cursor(args.capture_cursor);
    #[cfg(target_os = "linux")]
    {
        options.set_pipewire_capturer(true);
    }

    let mut capturer =
        DesktopCapturer::new(callback, options).expect("Failed to create desktop capturer");
    let sources = capturer.get_source_list();
    log::info!("Found {} sources", sources.len());

    let selected_source = sources.first().cloned();
    capturer.start_capture(selected_source);

    let now = tokio::time::Instant::now();
    while now.elapsed() < tokio::time::Duration::from_secs(30) {
        capturer.capture_frame();
        tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
    }
}
