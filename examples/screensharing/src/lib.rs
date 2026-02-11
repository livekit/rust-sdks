#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
mod test {
    use clap::{ArgAction, Parser};
    use livekit::options::{TrackPublishOptions, VideoCodec};
    use livekit::prelude::*;
    use livekit::track::{LocalTrack, LocalVideoTrack, TrackSource};
    use livekit::webrtc::desktop_capturer::{
        CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        DesktopFrame,
    };
    use livekit::webrtc::native::yuv_helper;
    use livekit::webrtc::prelude::{
        I420Buffer, RtcVideoSource, VideoBuffer, VideoFrame, VideoResolution, VideoRotation,
    };
    use livekit::webrtc::video_source::native::NativeVideoSource;
    use livekit_api::access_token;
    use std::env;
    use std::sync::mpsc::{self, RecvTimeoutError, Sender};
    use std::sync::{Arc, Condvar, Mutex};
    use std::thread;
    use std::time::Duration;
    use tokio::signal;

    #[derive(clap::ValueEnum, Clone, Debug)]
    enum SourceType {
        Screen,
        Window,
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        Generic,
    }

    impl From<SourceType> for DesktopCaptureSourceType {
        fn from(source: SourceType) -> Self {
            match source {
                SourceType::Screen => DesktopCaptureSourceType::Screen,
                SourceType::Window => DesktopCaptureSourceType::Window,
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                SourceType::Generic => DesktopCaptureSourceType::Generic,
            }
        }
    }

    enum CaptureCommand {
        Terminate,
    }

    type ResolutionSignal = Arc<(Mutex<Option<VideoResolution>>, Condvar)>;
    type VideoSourceSlot = Arc<Mutex<Option<NativeVideoSource>>>;

    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    struct Args {
        /// Capture the mouse cursor
        #[arg(long)]
        capture_cursor: bool,

        /// Capture a specific source type (screen, window, generic)
        #[arg(long)]
        capture_source_type: SourceType,

        /// Use system screen picker (macOS only)
        #[cfg(target_os = "macos")]
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        use_system_picker: bool,
    }

    pub async fn run() {
        env_logger::init();
        let args = Args::parse();

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

        let resolution_signal: ResolutionSignal = Arc::new((Mutex::new(None), Condvar::new()));
        let video_source_slot: VideoSourceSlot = Arc::new(Mutex::new(None));
        let capture_source_type = args.capture_source_type.clone();
        let (capture_cmd_tx, capture_handle) = spawn_capture_thread(
            args.capture_cursor,
            capture_source_type.into(),
            #[cfg(target_os = "macos")]
            args.use_system_picker,
            resolution_signal.clone(),
            video_source_slot.clone(),
        );

        let resolution = wait_for_resolution(&resolution_signal);
        log::info!("Detected capture resolution: {}x{}", resolution.width, resolution.height);

        let buffer_source = NativeVideoSource::new(resolution.clone());
        {
            let mut slot = video_source_slot.lock().unwrap();
            *slot = Some(buffer_source.clone());
        }

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

        log::info!("Screen sharing started. Press Ctrl+C to stop.");
        let ctrl_c = signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    log::info!("Ctrl+C received, stopping capture");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }

        let _ = capture_cmd_tx.send(CaptureCommand::Terminate);
        if let Err(err) = capture_handle.join() {
            log::error!("Capture thread join error: {:?}", err);
        }
    }

    fn wait_for_resolution(signal: &ResolutionSignal) -> VideoResolution {
        let (lock, cvar) = &**signal;
        let mut guard = lock.lock().unwrap();
        while guard.is_none() {
            guard = cvar.wait(guard).unwrap();
        }
        guard.clone().unwrap()
    }

    fn spawn_capture_thread(
        capture_cursor: bool,
        source_type: DesktopCaptureSourceType,
        #[cfg(target_os = "macos")] use_system_picker: bool,
        resolution_signal: ResolutionSignal,
        video_source_slot: VideoSourceSlot,
    ) -> (Sender<CaptureCommand>, thread::JoinHandle<()>) {
        let (command_tx, command_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            run_capture_loop(
                capture_cursor,
                source_type,
                #[cfg(target_os = "macos")]
                use_system_picker,
                resolution_signal,
                video_source_slot,
                command_rx,
            )
        });
        (command_tx, handle)
    }

    fn run_capture_loop(
        capture_cursor: bool,
        source_type: DesktopCaptureSourceType,
        #[cfg(target_os = "macos")] use_system_picker: bool,
        resolution_signal: ResolutionSignal,
        video_source_slot: VideoSourceSlot,
        command_rx: mpsc::Receiver<CaptureCommand>,
    ) {
        let callback = {
            let mut frame_buffer = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                buffer: I420Buffer::new(1, 1),
                timestamp_us: 0,
                user_timestamp_us: None,
            };
            move |result: Result<DesktopFrame, CaptureError>| {
                let frame = match result {
                    Ok(frame) => frame,
                    Err(CaptureError::Temporary) => {
                        log::debug!("Error temporary");
                        return;
                    }
                    Err(CaptureError::Permanent) => {
                        log::debug!("Error permanent");
                        return;
                    }
                };

                let width = frame.width();
                let height = frame.height();
                let stride = frame.stride();
                let data = frame.data();

                {
                    let (lock, cvar) = &*resolution_signal;
                    let mut guard = lock.lock().unwrap();
                    if guard.is_none() {
                        *guard =
                            Some(VideoResolution { width: width as u32, height: height as u32 });
                        cvar.notify_all();
                    }
                }

                let buffer_width = frame_buffer.buffer.width() as i32;
                let buffer_height = frame_buffer.buffer.height() as i32;
                if buffer_width != width || buffer_height != height {
                    frame_buffer.buffer = I420Buffer::new(width as u32, height as u32);
                }

                let (stride_y, stride_u, stride_v) = frame_buffer.buffer.strides();
                let (y_plane, u_plane, v_plane) = frame_buffer.buffer.data_mut();
                yuv_helper::argb_to_i420(
                    data, stride, y_plane, stride_y, u_plane, stride_u, v_plane, stride_v, width,
                    height,
                );

                let slot = video_source_slot.lock().unwrap();
                if let Some(source) = slot.as_ref() {
                    source.capture_frame(&frame_buffer);
                }
            }
        };

        let mut options = DesktopCapturerOptions::new(source_type);
        #[cfg(target_os = "macos")]
        {
            options.set_sck_system_picker(use_system_picker);
        }
        options.set_include_cursor(capture_cursor);

        let mut capturer =
            DesktopCapturer::new(options).expect("Failed to create desktop capturer");
        let sources = capturer.get_source_list();
        log::info!("Found {} sources", sources.len());

        let selected_source = sources.first().cloned();
        capturer.start_capture(selected_source, callback);

        loop {
            match command_rx.recv_timeout(Duration::from_millis(16)) {
                Ok(CaptureCommand::Terminate) => {
                    log::info!("Capture thread received terminate message");
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    capturer.capture_frame();
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        log::info!("Capture loop exiting");
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub use test::run;

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub async fn run() {
    let _ = env_logger::try_init();
    log::info!("screensharing example is only available on Linux; skipping.");
}
