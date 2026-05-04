use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit_api::access_token;
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

// Usage:
//   cargo run -p basic_room -- --list-devices                         # List audio devices and exit
//   cargo run -p basic_room -- --platform-audio                       # Publish microphone using PlatformAudio
//   cargo run -p basic_room -- --platform-audio-and-file <path.wav>   # Publish both mic + WAV file
//   cargo run -p basic_room -- --file <path.wav>                      # Publish just WAV file (no mic)
//   cargo run -p basic_room -- --room <room-name>                     # Specify room name (default: my-room)
//   cargo run -p basic_room                                           # Connect without audio publishing

#[tokio::main]
async fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let list_devices = args.iter().any(|arg| arg == "--list-devices");
    let use_platform_audio = args.iter().any(|arg| arg == "--platform-audio");

    // Check for --platform-audio-and-file <path>
    let platform_audio_and_file_path = args
        .iter()
        .position(|arg| arg == "--platform-audio-and-file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());

    // Check for --file <path> (file only, no microphone)
    let file_only_path = args
        .iter()
        .position(|arg| arg == "--file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone());

    // Check for --room <name> (default: my-room)
    let room_name = args
        .iter()
        .position(|arg| arg == "--room")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.clone())
        .unwrap_or_else(|| "my-room".to_string());

    let use_platform_audio_and_file = platform_audio_and_file_path.is_some();
    let file_path = platform_audio_and_file_path.or(file_only_path.clone());

    // --list-devices: enumerate audio devices and exit
    if list_devices {
        let audio = match PlatformAudio::new() {
            Ok(audio) => audio,
            Err(e) => {
                eprintln!("Failed to initialize platform audio: {}", e);
                return;
            }
        };

        println!("Recording devices (microphones):");
        let recording_count = audio.recording_devices();
        if recording_count == 0 {
            println!("  (none)");
        } else {
            for i in 0..recording_count as u16 {
                println!("  [{}] {}", i, audio.recording_device_name(i));
            }
        }

        println!("\nPlayout devices (speakers):");
        let playout_count = audio.playout_devices();
        if playout_count == 0 {
            println!("  (none)");
        } else {
            for i in 0..playout_count as u16 {
                println!("  [{}] {}", i, audio.playout_device_name(i));
            }
        }

        println!("\nAudio processing:");
        println!("  Hardware AEC available: {}", audio.is_hardware_aec_available());
        println!("  Hardware AGC available: {}", audio.is_hardware_agc_available());
        println!("  Hardware NS available:  {}", audio.is_hardware_ns_available());

        return;
    }

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    // Determine what to publish
    let publish_mic = use_platform_audio || use_platform_audio_and_file;

    // Create PlatformAudio if needed (must be created BEFORE connecting to room)
    let platform_audio = if publish_mic {
        let audio = PlatformAudio::new().expect("Failed to initialize platform audio");
        log::info!("Platform audio initialized");

        let recording_count = audio.recording_devices();
        let playout_count = audio.playout_devices();

        log::info!("Recording devices: {}", recording_count);
        for i in 0..recording_count as u16 {
            log::info!("  [{}] {}", i, audio.recording_device_name(i));
        }

        log::info!("Playout devices: {}", playout_count);
        for i in 0..playout_count as u16 {
            log::info!("  [{}] {}", i, audio.playout_device_name(i));
        }

        if recording_count > 0 {
            audio.set_recording_device(0).expect("Failed to set recording device");
        }
        if playout_count > 0 {
            audio.set_playout_device(0).expect("Failed to set playout device");
        }

        audio
            .configure_audio_processing(AudioProcessingOptions {
                echo_cancellation: true,
                noise_suppression: true,
                auto_gain_control: true,
                prefer_hardware_processing: false,
            })
            .expect("Failed to configure audio processing");

        Some(audio)
    } else {
        None
    };

    // Load WAV file if specified
    // Note: ADM recording is disabled by default, so when using --file mode (NativeAudioSource only),
    // the microphone will not be activated. It's only enabled when PlatformAudio::new() is called.
    let wav_data = if let Some(ref path) = file_path {
        Some(load_wav_file(path).expect("Failed to load WAV file"))
    } else {
        None
    };

    // Create NativeAudioSource for file playback if needed
    // Use queue_size_ms > 0 for buffered path - internal AudioTask delivers frames every 10ms
    // This should provide more consistent timing when ADM recording is also active
    let file_source = if let Some(ref wav) = wav_data {
        log::info!(
            "Creating NativeAudioSource: sample_rate={}, channels={}",
            wav.sample_rate,
            wav.channels
        );
        Some(NativeAudioSource::new(
            AudioSourceOptions::default(),
            wav.sample_rate,
            wav.channels,
            0, // Fast path: direct delivery to avoid race condition with ADM
        ))
    } else {
        None
    };

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.clone(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap();

    log::info!("Joining room: {}", room_name);

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await.unwrap();
    log::info!("Connected to room: {}", room.name());

    // DIAGNOSTIC: Publish file audio track FIRST (before microphone)
    // This helps diagnose if the first track sets global audio configuration
    let running = Arc::new(AtomicBool::new(true));
    let file_task = if let (Some(source), Some(wav)) = (file_source.as_ref(), wav_data.clone()) {
        let track = LocalAudioTrack::create_audio_track(
            "file_audio",
            RtcAudioSource::Native(source.clone()),
        );

        // Ensure the track is unmuted before publishing
        track.unmute();
        log::info!(
            "File track state before publish: enabled={}, muted={}",
            track.is_enabled(),
            track.is_muted()
        );

        let publication = room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track.clone()),
                TrackPublishOptions { source: TrackSource::Unknown, ..Default::default() },
            )
            .await
            .expect("Failed to publish file audio track");

        // Ensure track is enabled and unmuted after publishing
        track.enable();
        track.unmute();
        log::info!(
            "File track state after publish: enabled={}, muted={}, publication_muted={}",
            track.is_enabled(),
            track.is_muted(),
            publication.is_muted()
        );

        log::info!(
            "Published file audio track FIRST: {} Hz, {} channels, {} samples",
            wav.sample_rate,
            wav.channels,
            wav.samples.len()
        );

        // Wait for the file track to be fully set up before publishing microphone
        log::info!("Waiting 500ms for file audio track setup before publishing mic...");
        tokio::time::sleep(Duration::from_millis(500)).await;

        let source_clone = source.clone();
        let running_clone = running.clone();
        Some(tokio::spawn(async move {
            // Additional wait for playback to ensure everything is connected
            log::info!("Starting WAV playback...");
            play_wav_file(source_clone, wav, running_clone).await;
        }))
    } else {
        None
    };

    // Publish microphone track SECOND (after file track is set up)
    //
    // DIAGNOSTIC FINDINGS:
    // - SKIP_MIC_PUBLISH=1 still crashes because ADM recording is still active
    // - The race condition is between ADM's audio thread and NativeAudioSource's tokio thread
    // - To avoid the crash, use --file mode (no PlatformAudio, no ADM recording)
    //
    let skip_mic_publish = std::env::var("SKIP_MIC_PUBLISH").is_ok();

    if let Some(ref audio) = platform_audio {
        if skip_mic_publish {
            log::warn!("DIAGNOSTIC: PlatformAudio is active (ADM recording enabled) but NOT publishing mic track");
            log::warn!("If audio still plays at wrong speed, the issue is ADM configuration");
            log::warn!("If audio plays correctly, the issue is the device audio track publishing");
        } else {
            let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());

            log::info!("Publishing microphone track SECOND (after file track)...");
            room.local_participant()
                .publish_track(
                    LocalTrack::Audio(track),
                    TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
                )
                .await
                .expect("Failed to publish microphone track");

            log::info!("Published microphone track using PlatformAudio");

            if file_task.is_some() {
                log::info!("Both tracks published: file (48kHz) FIRST, then microphone");
                log::warn!(
                    "WARNING: Publishing both simultaneously may cause sample rate conflicts!"
                );
            }
        }
    }

    room.local_participant()
        .publish_data(DataPacket {
            payload: "Hello world".to_owned().into_bytes(),
            reliable: true,
            ..Default::default()
        })
        .await
        .unwrap();

    log::info!("Entering event loop - press Ctrl+C to stop");

    // Handle Ctrl+C gracefully
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        log::info!("Received Ctrl+C signal");
    };

    tokio::select! {
        _ = ctrl_c => {
            log::info!("Shutting down gracefully...");
        }
        _ = async {
            while let Some(msg) = rx.recv().await {
                log::info!("Event: {:?}", msg);
            }
        } => {
            log::info!("Event loop ended");
        }
    }

    // Stop playback task
    log::info!("Stopping playback...");
    running.store(false, Ordering::SeqCst);
    if let Some(task) = file_task {
        log::info!("Waiting for playback task to finish...");
        let _ = task.await;
    }

    // Disconnect from the room gracefully
    log::info!("Disconnecting from room...");
    room.close().await;
    log::info!("Disconnected. Goodbye!");
}

#[derive(Clone)]
struct WavData {
    sample_rate: u32,
    channels: u32,
    samples: Vec<i16>,
}

fn load_wav_file<P: AsRef<Path>>(path: P) -> Result<WavData, Box<dyn std::error::Error>> {
    let path = path.as_ref();
    log::info!("Loading WAV file: {}", path.display());

    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    log::info!(
        "WAV spec: {} Hz, {} channels, {} bits, {:?}",
        spec.sample_rate,
        spec.channels,
        spec.bits_per_sample,
        spec.sample_format
    );

    let samples: Vec<i16> = match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample == 16 {
                reader.into_samples::<i16>().filter_map(|s| s.ok()).collect()
            } else if spec.bits_per_sample == 32 {
                reader
                    .into_samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| (s >> 16) as i16)
                    .collect()
            } else if spec.bits_per_sample == 8 {
                reader
                    .into_samples::<i8>()
                    .filter_map(|s| s.ok())
                    .map(|s| (s as i16) << 8)
                    .collect()
            } else {
                return Err(format!("Unsupported bit depth: {}", spec.bits_per_sample).into());
            }
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .map(|s| (s * i16::MAX as f32) as i16)
            .collect(),
    };

    log::info!("Loaded {} samples from WAV file", samples.len());

    Ok(WavData { sample_rate: spec.sample_rate, channels: spec.channels as u32, samples })
}

async fn play_wav_file(source: NativeAudioSource, wav: WavData, running: Arc<AtomicBool>) {
    log::info!("=== WAV PLAYBACK TASK STARTED ===");

    let samples_per_channel_per_frame = (wav.sample_rate / 100) as usize; // 10ms frames
    let samples_per_frame = samples_per_channel_per_frame * wav.channels as usize;
    let total_duration_secs =
        wav.samples.len() as f64 / (wav.sample_rate as f64 * wav.channels as f64);

    log::info!(
        "WAV playback config: sample_rate={}, channels={}, samples_per_channel_per_frame={}, samples_per_frame={}, total_samples={}, duration={:.2}s",
        wav.sample_rate,
        wav.channels,
        samples_per_channel_per_frame,
        samples_per_frame,
        wav.samples.len(),
        total_duration_secs
    );
    log::info!(
        "NativeAudioSource config: sample_rate={}, num_channels={}",
        source.sample_rate(),
        source.num_channels()
    );

    // Use interval for accurate timing instead of sleep
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut position = 0;
    let mut frame_count = 0u64;
    let start_time = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        interval.tick().await;

        let end = (position + samples_per_frame).min(wav.samples.len());
        let frame_samples: Vec<i16> = if end > position {
            wav.samples[position..end].to_vec()
        } else {
            // Restart from beginning (loop)
            position = 0;
            let end = samples_per_frame.min(wav.samples.len());
            wav.samples[0..end].to_vec()
        };

        // Pad with silence if needed
        let mut padded = frame_samples;
        while padded.len() < samples_per_frame {
            padded.push(0);
        }

        // Check if audio data is not silent (first few frames)
        if frame_count < 5 {
            let max_sample = padded.iter().map(|s| s.abs()).max().unwrap_or(0);
            let avg_sample: i32 =
                padded.iter().map(|s| (*s as i32).abs()).sum::<i32>() / padded.len() as i32;
            log::info!(
                "Frame {} audio data: max={}, avg={}, first_samples={:?}",
                frame_count,
                max_sample,
                avg_sample,
                &padded[..8.min(padded.len())]
            );
        }

        let frame = livekit::webrtc::audio_frame::AudioFrame {
            data: padded.into(),
            sample_rate: wav.sample_rate,
            num_channels: wav.channels,
            samples_per_channel: samples_per_channel_per_frame as u32,
        };

        match source.capture_frame(&frame).await {
            Ok(()) => {
                // Log first 10 frames to verify playback is working
                if frame_count < 10 {
                    log::info!(
                        "Frame {} captured successfully (position={}, sample_rate={}, channels={}, samples_per_ch={})",
                        frame_count, position, frame.sample_rate, frame.num_channels, frame.samples_per_channel
                    );
                }
            }
            Err(e) => {
                log::warn!("Failed to capture frame {}: {}", frame_count, e);
            }
        }

        position += samples_per_frame;
        frame_count += 1;

        // Log timing every 100 frames (1 second)
        if frame_count % 100 == 0 {
            let elapsed = start_time.elapsed();
            let expected_ms = frame_count * 10;
            let actual_ms = elapsed.as_millis() as u64;
            let drift_ms = actual_ms as i64 - expected_ms as i64;
            log::info!(
                "Playback progress: frame={}, elapsed={}ms, expected={}ms, drift={}ms",
                frame_count,
                actual_ms,
                expected_ms,
                drift_ms
            );
        }

        if position >= wav.samples.len() {
            position = 0; // Loop
            log::info!(
                "WAV playback looping after {} frames ({:.1}s)",
                frame_count,
                frame_count as f64 * 0.01
            );
        }
    }

    log::info!(
        "=== WAV PLAYBACK TASK STOPPED after {} frames ({:.1}s) ===",
        frame_count,
        frame_count as f64 * 0.01
    );
}
