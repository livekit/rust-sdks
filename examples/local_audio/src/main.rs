mod db_meter;
mod audio_mixer;
mod audio_capture;
mod audio_playback;

use anyhow::{anyhow, Result};
use audio_capture::AudioCapture;
use audio_mixer::AudioMixer;
use audio_playback::AudioPlayback;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleRate, StreamConfig};
use db_meter::display_db_meter;
use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::native::NativeAudioSource,
        audio_stream::native::NativeAudioStream,
        prelude::{AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomEvent, RoomOptions,
};
use livekit_api::access_token;
use log::{debug, error, info};
use std::{
    env,
    sync::Arc,
};
use tokio::sync::mpsc;
use futures_util::StreamExt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available audio devices and exit
    #[arg(short, long)]
    list_devices: bool,

    /// Audio input device name to use (default: system default)
    #[arg(short = 'i', long)]
    input_device: Option<String>,

    /// Audio output device name to use (default: system default)
    #[arg(short = 'o', long)]
    output_device: Option<String>,

    /// Sample rate in Hz (default: 48000)
    #[arg(short, long, default_value_t = 48000)]
    sample_rate: u32,

    /// Number of channels (default: 1)
    #[arg(short, long, default_value_t = 1)]
    channels: u32,

    /// Enable echo cancellation
    #[arg(long, default_value_t = true)]
    echo_cancellation: bool,

    /// Enable noise suppression
    #[arg(long, default_value_t = true)]
    noise_suppression: bool,

    /// Enable auto gain control
    #[arg(long, default_value_t = true)]
    auto_gain_control: bool,

    /// Disable audio playback (capture only)
    #[arg(long, default_value_t = false)]
    no_playback: bool,

    /// Master playback volume (0.0 to 1.0, default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    volume: f32,

    /// LiveKit participant identity (default: "rust-audio-streamer")
    #[arg(long, default_value = "rust-audio-streamer")]
    identity: String,

    /// LiveKit room name to join (default: "audio-room")
    #[arg(long, default_value = "audio-room")]
    room_name: String,

    /// LiveKit server URL (can also be set via LIVEKIT_URL environment variable)
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (can also be set via LIVEKIT_API_KEY environment variable)
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret (can also be set via LIVEKIT_API_SECRET environment variable)
    #[arg(long)]
    api_secret: Option<String>,
}







fn list_audio_devices() -> Result<()> {
    let host = cpal::default_host();
    
    println!("Available audio input devices:");
    println!("─────────────────────────────");

    let input_devices = host.input_devices()?;
    
    for (i, device) in input_devices.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("{}. {}", i + 1, name);
        
        if let Ok(config) = device.default_input_config() {
            println!("   └─ Sample rate: {} Hz", config.sample_rate().0);
            println!("   └─ Channels: {}", config.channels());
            println!("   └─ Sample format: {:?}", config.sample_format());
        }
        println!();
    }

    // Show default input device
    if let Some(device) = host.default_input_device() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("Default input device: {}", name);
    }

    println!("\nAvailable audio output devices:");
    println!("─────────────────────────────");

    let output_devices = host.output_devices()?;
    
    for (i, device) in output_devices.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("{}. {}", i + 1, name);
        
        if let Ok(config) = device.default_output_config() {
            println!("   └─ Sample rate: {} Hz", config.sample_rate().0);
            println!("   └─ Channels: {}", config.channels());
            println!("   └─ Sample format: {:?}", config.sample_format());
        }
        println!();
    }

    // Show default output device
    if let Some(device) = host.default_output_device() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("Default output device: {}", name);
    }

    Ok(())
}

fn find_input_device_by_name(name: &str) -> Result<Device> {
    let host = cpal::default_host();
    let devices = host.input_devices()?;
    
    for device in devices {
        if let Ok(device_name) = device.name() {
            if device_name.contains(name) {
                return Ok(device);
            }
        }
    }
    
    Err(anyhow!("Input device '{}' not found", name))
}

fn find_output_device_by_name(name: &str) -> Result<Device> {
    let host = cpal::default_host();
    let devices = host.output_devices()?;
    
    for device in devices {
        if let Ok(device_name) = device.name() {
            if device_name.contains(name) {
                return Ok(device);
            }
        }
    }
    
    Err(anyhow!("Output device '{}' not found", name))
}

async fn stream_audio_to_livekit(
    mut audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    livekit_source: NativeAudioSource,
    sample_rate: u32,
    channels: u32,
) -> Result<()> {
    let mut buffer = Vec::new();
    let samples_per_10ms = (sample_rate as usize * channels as usize) / 100;
    
    info!(
        "Starting LiveKit audio streaming ({}Hz, {} channels, {} samples per 10ms)",
        sample_rate, channels, samples_per_10ms
    );

    while let Some(audio_data) = audio_rx.recv().await {
        buffer.extend_from_slice(&audio_data);

        // Send 10ms chunks to LiveKit
        while buffer.len() >= samples_per_10ms {
            let chunk: Vec<i16> = buffer.drain(..samples_per_10ms).collect();
            
            let audio_frame = AudioFrame {
                data: chunk.into(),
                sample_rate,
                num_channels: channels,
                samples_per_channel: (samples_per_10ms / channels as usize) as u32,
            };

            if let Err(e) = livekit_source.capture_frame(&audio_frame).await {
                error!("Failed to send audio frame to LiveKit: {}", e);
            }
        }
    }

    Ok(())
}

async fn handle_remote_audio_streams(
    room: Arc<Room>,
    mixer: AudioMixer,
    sample_rate: u32,
    channels: u32,
) -> Result<()> {
    let mut room_events = room.subscribe();

    info!("Starting remote audio stream handler");

    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::ParticipantConnected(participant) => {
                info!("Participant connected: {} ({})", participant.identity(), participant.name());
            }
            
            RoomEvent::TrackPublished { participant, publication } => {
                info!("Track published by {}: {} ({:?})", 
                    participant.identity(), publication.name(), publication.kind());
            }
            
            RoomEvent::TrackSubscribed { track, participant, .. } => {
                info!("Track subscribed from {}: {} ({:?})", 
                    participant.identity(), track.name(), track.kind());
                    
                if let livekit::track::RemoteTrack::Audio(audio_track) = track {
                    let participant_identity = participant.identity().to_string();
                    info!("Setting up audio stream for participant: {}", participant_identity);

                    // Create audio stream for this remote track
                    let mut audio_stream = NativeAudioStream::new(
                        audio_track.rtc_track(),
                        sample_rate as i32,
                        channels as i32,
                    );

                    // Start processing audio frames from this participant
                    let stream_key = participant_identity.clone();
                    let mixer_clone = mixer.clone();

                    tokio::spawn(async move {
                        info!("Starting audio stream processing for participant: {}", stream_key);
                        
                        while let Some(audio_frame) = audio_stream.next().await {
                            // Add this participant's audio to the mixer
                            mixer_clone.add_audio_data(audio_frame.data.as_ref());
                            
                            debug!("Received audio frame from {}: {} samples, {} channels, {} Hz, buffer size: {}", 
                                stream_key, audio_frame.data.len(), audio_frame.num_channels, 
                                audio_frame.sample_rate, mixer_clone.buffer_size());
                        }
                        
                        info!("Audio stream ended for participant: {}", stream_key);
                    });
                }
            }
            
            RoomEvent::TrackUnsubscribed { track, participant, .. } => {
                info!("Track unsubscribed from {}: {} ({:?})", 
                    participant.identity(), track.name(), track.kind());
                    
                if let livekit::track::RemoteTrack::Audio(_) = track {
                    let participant_identity = participant.identity().to_string();
                    info!("Stopping audio stream for participant: {}", participant_identity);
                    
                    // Audio stream will be automatically cleaned up when the task ends
                }
            }

            RoomEvent::ParticipantDisconnected(participant) => {
                let participant_identity = participant.identity().to_string();
                info!("Participant disconnected: {}", participant_identity);
                
                // Audio stream will be automatically cleaned up when the task ends
            }

            _ => {
                // Handle other room events as needed
                debug!("Room event: {:?}", event);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    if args.list_devices {
        return list_audio_devices();
    }

    // Validate volume parameter
    if args.volume < 0.0 || args.volume > 1.0 {
        return Err(anyhow!("Volume must be between 0.0 and 1.0"));
    }

    // Get LiveKit connection details from CLI arguments or environment variables
    let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable");
    let api_key = args.api_key.or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LiveKit API key must be provided via --api-key argument or LIVEKIT_API_KEY environment variable");
    let api_secret = args.api_secret.or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LiveKit API secret must be provided via --api-secret argument or LIVEKIT_API_SECRET environment variable");
    
    // Create access token
    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            ..Default::default()
        })
        .to_jwt()?;
        
    // Connect to LiveKit room with auto-subscribe enabled
    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected to room: {} - {}", room.name(), room.sid().await);

    // Set up audio input device
    let host = cpal::default_host();
    let input_device = if let Some(device_name) = &args.input_device {
        info!("Looking for input device: {}", device_name);
        find_input_device_by_name(device_name)?
    } else {
        info!("Using default input device");
        host.default_input_device()
            .ok_or_else(|| anyhow!("No default input device available"))?
    };

    let input_device_name = input_device.name().unwrap_or_else(|_| "Unknown".to_string());
    info!("Using audio input device: {}", input_device_name);

    // Set up audio output device (if playback is enabled)
    let output_device = if !args.no_playback {
        let device = if let Some(device_name) = &args.output_device {
            info!("Looking for output device: {}", device_name);
            Some(find_output_device_by_name(device_name)?)
        } else {
            info!("Using default output device");
            host.default_output_device()
        };

        if let Some(ref device) = device {
            let output_device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            info!("Using audio output device: {}", output_device_name);
        }
        
        device
    } else {
        info!("Audio playback disabled");
        None
    };

    // Configure audio capture
    let input_supported_config = input_device.default_input_config()?;
    let input_config = StreamConfig {
        channels: args.channels as u16,
        sample_rate: SampleRate(args.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };
    
    info!(
        "Audio input config: {}Hz, {} channels, {:?}",
        input_config.sample_rate.0,
        input_config.channels,
        input_supported_config.sample_format()
    );

    // Configure audio playback (if enabled)
    let output_config = if output_device.is_some() {
        let config = StreamConfig {
            channels: args.channels as u16,
            sample_rate: SampleRate(args.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        
        info!(
            "Audio output config: {}Hz, {} channels",
            config.sample_rate.0,
            config.channels,
        );
        
        Some(config)
    } else {
        None
    };

    // Create LiveKit audio source with audio processing options
    let audio_options = AudioSourceOptions {
        echo_cancellation: args.echo_cancellation,
        noise_suppression: args.noise_suppression,
        auto_gain_control: args.auto_gain_control,
    };

    info!(
        "Audio processing - Echo cancellation: {}, Noise suppression: {}, Auto gain control: {}",
        audio_options.echo_cancellation,
        audio_options.noise_suppression,
        audio_options.auto_gain_control
    );

    let livekit_source = NativeAudioSource::new(
        audio_options,
        args.sample_rate,
        args.channels,
        1000, // 1 second buffer
    );

    // Create and publish audio track
    let track = LocalAudioTrack::create_audio_track(
        "microphone",
        RtcAudioSource::Native(livekit_source.clone()),
    );

    room.local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                ..Default::default()
            },
        )
        .await?;

    info!("Audio track published to LiveKit successfully");

    // Set up audio capture and streaming
    let (audio_tx, audio_rx) = mpsc::unbounded_channel();
    
    // Set up dB meter
    let (db_tx, db_rx) = mpsc::unbounded_channel();
    
    // Start dB meter display
    let db_meter_task = tokio::spawn(display_db_meter(db_rx));
    
    // Start audio capture
    let _audio_capture = AudioCapture::new(
        input_device,
        input_config,
        input_supported_config.sample_format(),
        audio_tx,
        Some(db_tx)
    ).await?;
    
    // Start streaming to LiveKit
    let streaming_task = tokio::spawn(stream_audio_to_livekit(
        audio_rx,
        livekit_source,
        args.sample_rate,
        args.channels,
    ));

    // Set up audio playback (if enabled)
    let _audio_playback = if let (Some(output_device), Some(output_config)) = (output_device, output_config) {
        // Create audio mixer for combining participant audio streams
        let mixer = AudioMixer::new(args.sample_rate, args.channels, args.volume);
        
        // Start handling remote audio streams
        let room_clone = room.clone();
        let mixer_clone = mixer.clone();
        let remote_audio_task = tokio::spawn(handle_remote_audio_streams(
            room_clone,
            mixer_clone,
            args.sample_rate,
            args.channels,
        ));

        // Get output device format
        let output_supported_config = output_device.default_output_config()?;
        
        // Start audio playback
        let playback = AudioPlayback::new(
            output_device,
            output_config,
            output_supported_config.sample_format(),
            mixer,
        ).await?;

        info!("Audio playback enabled with volume: {:.1}%", args.volume * 100.0);
        
        // Store the remote audio task handle for proper cleanup
        let remote_audio_task_handle = remote_audio_task;
        
        // Keep the task alive by storing it in a variable that won't be dropped
        tokio::spawn(async move {
            let _ = remote_audio_task_handle.await;
        });
        
        Some(playback)
    } else {
        None
    };

    info!(
        "Audio streaming started. Microphone: {}, Playback: {}. Press Ctrl+C to stop.",
        if args.input_device.is_some() { "Custom" } else { "Default" },
        if _audio_playback.is_some() { "Enabled" } else { "Disabled" }
    );

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("\nShutting down...");

    // Clean shutdown
    streaming_task.abort();
    db_meter_task.abort();
    room.close().await?;

    Ok(())
} 