use anyhow::{anyhow, Result};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig, Sample, FromSample};
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
use log::{debug, error, info, warn};
use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::mpsc;
use futures_util::StreamExt;

// Add dB meter related constants and functions
const DB_METER_UPDATE_INTERVAL_MS: u64 = 50; // Update every 50ms
const DB_METER_WIDTH: usize = 40; // Width of the dB meter bar

fn calculate_db_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return -60.0; // Very quiet
    }
    
    // Calculate RMS
    let sum_squares: f64 = samples.iter()
        .map(|&sample| {
            let normalized = sample as f64 / i16::MAX as f64;
            normalized * normalized
        })
        .sum();
    
    let rms = (sum_squares / samples.len() as f64).sqrt();
    
    // Convert to dB (20 * log10(rms))
    if rms > 0.0 {
        20.0 * rms.log10() as f32
    } else {
        -60.0 // Very quiet
    }
}

fn format_db_meter(db_level: f32) -> String {
    let db_clamped = db_level.clamp(-60.0, 0.0);
    let normalized = (db_clamped + 60.0) / 60.0; // Normalize to 0.0-1.0
    let filled_width = (normalized * DB_METER_WIDTH as f32) as usize;
    
    let mut meter = String::new();
    meter.push_str("\r"); // Return to start of line
    meter.push_str("Mic Level: ");
    
    // Add the dB value
    meter.push_str(&format!("{:>5.1} dB ", db_level));
    
    // Add the visual meter
    meter.push('[');
    for i in 0..DB_METER_WIDTH {
        if i < filled_width {
            if i < DB_METER_WIDTH * 2 / 3 {
                meter.push('█'); // Full block for low/medium levels
            } else if i < DB_METER_WIDTH * 9 / 10 {
                meter.push('▓'); // Medium block for high levels
            } else {
                meter.push('▒'); // Light block for very high levels (clipping warning)
            }
        } else {
            meter.push('░'); // Light shade for empty
        }
    }
    meter.push(']');

    meter
}

async fn display_db_meter(mut db_rx: mpsc::UnboundedReceiver<f32>) -> Result<()> {
    let mut last_update = std::time::Instant::now();
    let mut current_db = -60.0f32;
    
    println!("\nLocal Audio Level");
    println!("────────────────────────────────────────");
    
    loop {
        tokio::select! {
            db_level = db_rx.recv() => {
                if let Some(db) = db_level {
                    current_db = db;
                    
                    // Update display at regular intervals
                    if last_update.elapsed().as_millis() >= DB_METER_UPDATE_INTERVAL_MS as u128 {
                        print!("{}", format_db_meter(current_db));
                        use std::io::{self, Write};
                        io::stdout().flush().unwrap();
                        last_update = std::time::Instant::now();
                    }
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(DB_METER_UPDATE_INTERVAL_MS)) => {
                // Update display even if no new data
                print!("{}", format_db_meter(current_db));
                use std::io::{self, Write};
                io::stdout().flush().unwrap();
            }
        }
    }
    
    Ok(())
}

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
    #[arg(long)]
    no_playback: bool,

    /// Master playback volume (0.0 to 1.0, default: 1.0)
    #[arg(long, default_value_t = 1.0)]
    volume: f32,

    /// LiveKit participant identity (default: "audio-streamer")
    #[arg(long, default_value = "audio-streamer")]
    identity: String,

    /// LiveKit room name to join (default: "audio-room")
    #[arg(long, default_value = "audio-room")]
    room_name: String,
}

struct AudioCapture {
    _stream: Stream,
    is_running: Arc<AtomicBool>,
}

impl AudioCapture {
    async fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        db_tx: Option<mpsc::UnboundedSender<f32>>,
    ) -> Result<Self> {
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();

        let stream = match sample_format {
            SampleFormat::F32 => Self::create_input_stream::<f32>(device, config, audio_tx, db_tx, is_running_clone)?,
            SampleFormat::I16 => Self::create_input_stream::<i16>(device, config, audio_tx, db_tx, is_running_clone)?,
            SampleFormat::U16 => Self::create_input_stream::<u16>(device, config, audio_tx, db_tx, is_running_clone)?,
            sample_format => {
                return Err(anyhow!("Unsupported sample format: {:?}", sample_format));
            }
        };

        stream.play()?;
        info!("Audio capture stream started");

        Ok(AudioCapture {
            _stream: stream,
            is_running,
        })
    }

    fn create_input_stream<T>(
        device: Device,
        config: StreamConfig,
        audio_tx: mpsc::UnboundedSender<Vec<i16>>,
        db_tx: Option<mpsc::UnboundedSender<f32>>,
        is_running: Arc<AtomicBool>,
    ) -> Result<Stream>
    where
        T: SizedSample + Send + 'static,
    {
        let stream = device.build_input_stream(
            &config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !is_running.load(Ordering::Relaxed) {
                    return;
                }

                let converted: Vec<i16> = data.iter().map(|&sample| {
                    Self::convert_sample_to_i16(sample)
                }).collect();
                
                // Calculate and send dB level if channel is available
                if let Some(ref db_sender) = db_tx {
                    let db_level = calculate_db_level(&converted);
                    if let Err(e) = db_sender.send(db_level) {
                        warn!("Failed to send dB level: {}", e);
                    }
                }
                
                if let Err(e) = audio_tx.send(converted) {
                    warn!("Failed to send audio data: {}", e);
                }
            },
            move |err| {
                error!("Audio input stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    fn convert_sample_to_i16<T: SizedSample>(sample: T) -> i16 {
        if std::mem::size_of::<T>() == std::mem::size_of::<f32>() {
            let sample_f32 = unsafe { std::mem::transmute_copy::<T, f32>(&sample) };
            (sample_f32.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        } else if std::mem::size_of::<T>() == std::mem::size_of::<i16>() {
            unsafe { std::mem::transmute_copy::<T, i16>(&sample) }
        } else if std::mem::size_of::<T>() == std::mem::size_of::<u16>() {
            let sample_u16 = unsafe { std::mem::transmute_copy::<T, u16>(&sample) };
            ((sample_u16 as i32) - (u16::MAX as i32 / 2)) as i16
        } else {
            0
        }
    }

    fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Clone)]
struct AudioMixer {
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    channels: u32,
    volume: f32,
}

impl AudioMixer {
    fn new(sample_rate: u32, channels: u32, volume: f32) -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate,
            channels,
            volume: volume.clamp(0.0, 1.0),
        }
    }

    fn add_audio_data(&self, data: &[i16]) {
        let mut samples = self.samples.lock().unwrap();
        
        if samples.len() < data.len() {
            samples.resize(data.len(), 0);
        }

        // Mix the audio by adding samples together with volume scaling
        for (i, &sample) in data.iter().enumerate() {
            if i < samples.len() {
                let mixed = samples[i] as i32 + ((sample as f32 * self.volume) as i32);
                samples[i] = mixed.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
        }
    }

    fn get_and_clear_samples(&self) -> Vec<i16> {
        let mut samples = self.samples.lock().unwrap();
        let result = samples.clone();
        samples.clear();
        result
    }
}

struct AudioPlayback {
    _stream: Stream,
    is_running: Arc<AtomicBool>,
}

impl AudioPlayback {
    async fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        mixer: AudioMixer,
    ) -> Result<Self> {
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = is_running.clone();

        let stream = match sample_format {
            SampleFormat::F32 => Self::create_output_stream::<f32>(device, config, mixer, is_running_clone)?,
            SampleFormat::I16 => Self::create_output_stream::<i16>(device, config, mixer, is_running_clone)?,
            SampleFormat::U16 => Self::create_output_stream::<u16>(device, config, mixer, is_running_clone)?,
            sample_format => {
                return Err(anyhow!("Unsupported sample format: {:?}", sample_format));
            }
        };

        stream.play()?;
        info!("Audio playback stream started");

        Ok(AudioPlayback {
            _stream: stream,
            is_running,
        })
    }

    fn create_output_stream<T>(
        device: Device,
        config: StreamConfig,
        mixer: AudioMixer,
        is_running: Arc<AtomicBool>,
    ) -> Result<Stream>
    where
        T: SizedSample + Sample + Send + 'static + FromSample<f32>,
    {
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !is_running.load(Ordering::Relaxed) {
                    // Fill with silence if not running
                    for sample in data.iter_mut() {
                        *sample = Sample::from_sample(0.0f32);
                    }
                    return;
                }

                let mixed_samples = mixer.get_and_clear_samples();
                
                // Convert mixed i16 samples to output format
                for (i, sample) in data.iter_mut().enumerate() {
                    if i < mixed_samples.len() {
                        *sample = Self::convert_i16_to_sample::<T>(mixed_samples[i]);
                    } else {
                        *sample = Sample::from_sample(0.0f32); // Silence for missing samples
                    }
                }
            },
            move |err| {
                error!("Audio output stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    fn convert_i16_to_sample<T: SizedSample + Sample + FromSample<f32>>(sample: i16) -> T {
        if std::mem::size_of::<T>() == std::mem::size_of::<f32>() {
            let sample_f32 = sample as f32 / i16::MAX as f32;
            unsafe { std::mem::transmute_copy::<f32, T>(&sample_f32) }
        } else if std::mem::size_of::<T>() == std::mem::size_of::<i16>() {
            unsafe { std::mem::transmute_copy::<i16, T>(&sample) }
        } else if std::mem::size_of::<T>() == std::mem::size_of::<u16>() {
            let sample_u16 = ((sample as i32) + (u16::MAX as i32 / 2)) as u16;
            unsafe { std::mem::transmute_copy::<u16, T>(&sample_u16) }
        } else {
            Sample::from_sample(0.0f32)
        }
    }

    fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

impl Drop for AudioPlayback {
    fn drop(&mut self) {
        self.stop();
    }
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
            RoomEvent::TrackSubscribed { track, participant, .. } => {
                if let livekit::track::RemoteTrack::Audio(audio_track) = track {
                    let participant_identity = participant.identity().to_string();
                    info!("Subscribed to audio track from participant: {}", participant_identity);

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
                            mixer_clone.add_audio_data(&audio_frame.data);
                        }
                        
                        info!("Audio stream ended for participant: {}", stream_key);
                    });
                }
            }
            
            RoomEvent::TrackUnsubscribed { track, participant, .. } => {
                if let livekit::track::RemoteTrack::Audio(_) = track {
                    let participant_identity = participant.identity().to_string();
                    info!("Unsubscribed from audio track from participant: {}", participant_identity);
                    
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

    // Get LiveKit connection details from environment
    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");
    
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
        
    // Connect to LiveKit room
    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let (room, _) = Room::connect(&url, &token, RoomOptions::default()).await?;
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

    info!("Audio track published to LiveKit");

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
        
        // Don't drop the remote audio task
        std::mem::forget(remote_audio_task);
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