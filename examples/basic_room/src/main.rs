use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::audio_source::RtcAudioSource;
use livekit_api::access_token;
use std::env;

// Usage:
//   cargo run -p basic_room -- --list-devices     # List audio devices and exit
//   cargo run -p basic_room -- --platform-adm     # Connect with Platform ADM (microphone capture)
//   cargo run -p basic_room                       # Connect with Synthetic ADM (default)

#[tokio::main]
async fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let list_devices = args.iter().any(|arg| arg == "--list-devices");
    let use_platform_adm = args.iter().any(|arg| arg == "--platform-adm");

    // --list-devices: enumerate audio devices and exit
    if list_devices {
        let audio = AudioManager::instance();
        audio.set_mode(AudioMode::Platform).expect("Failed to set Platform ADM mode");

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

        return;
    }

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    // Configure audio mode BEFORE connecting to the room
    if use_platform_adm {
        let audio = AudioManager::instance();

        // Enable Platform ADM mode
        audio.set_mode(AudioMode::Platform).expect("Failed to set Platform ADM mode");
        log::info!("Platform ADM mode enabled");

        // Enumerate available devices
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

        // Use default devices (index 0)
        if recording_count > 0 {
            audio.set_recording_device(0).expect("Failed to set recording device");
        }
        if playout_count > 0 {
            audio.set_playout_device(0).expect("Failed to set playout device");
        }
    }

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "my-room".to_string(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap();

    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default())
        .await
        .unwrap();
    log::info!("Connected to room: {}", room.name());

    // Publish microphone track if Platform ADM mode is enabled
    if use_platform_adm {
        // Create a track using Device source (Platform ADM handles capture automatically)
        let track = LocalAudioTrack::create_audio_track("microphone", RtcAudioSource::Device);

        room.local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .expect("Failed to publish audio track");

        log::info!("Published microphone track using Platform ADM");
    }

    room.local_participant()
        .publish_data(DataPacket {
            payload: "Hello world".to_owned().into_bytes(),
            reliable: true,
            ..Default::default()
        })
        .await
        .unwrap();

    while let Some(msg) = rx.recv().await {
        log::info!("Event: {:?}", msg);
    }
}
