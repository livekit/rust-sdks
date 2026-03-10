mod audio_capture;
mod audio_mixer;
mod audio_playback;
mod latency;

use anyhow::{anyhow, Result};
use audio_capture::AudioCapture;
use audio_mixer::AudioMixer;
use audio_playback::AudioPlayback;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleRate, StreamConfig};
use futures_util::StreamExt;
use latency::{TurnLatencyBench, TurnLatencyBenchConfig};
use livekit::{
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, RemoteTrack, TrackSource},
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::native::NativeAudioSource,
        audio_stream::native::NativeAudioStream,
        prelude::{AudioSourceOptions, RtcAudioSource},
    },
    Room, RoomEvent, RoomOptions,
};
use livekit_api::access_token;
use log::{debug, info};
use rand::Rng;
use std::env;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

const SAMPLE_RATE_HZ: u32 = 48_000;

fn generate_random_string(length: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                             abcdefghijklmnopqrstuvwxyz\
                             0123456789";
    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}


#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Audio-only LiveKit client with optional agent latency benchmark"
)]
struct Args {
    #[arg(long)]
    list_devices: bool,

    #[arg(long)]
    url: Option<String>,

    #[arg(long)]
    token: Option<String>,

    #[arg(long)]
    api_key: Option<String>,

    #[arg(long)]
    api_secret: Option<String>,

    #[arg(long, default_value = "test-room")]
    room_name: String,

    #[arg(long, default_value = "rust-agent-client")]
    identity: String,

    #[arg(long)]
    input_device: Option<String>,

    #[arg(long)]
    output_device: Option<String>,

    #[arg(long, default_value_t = SAMPLE_RATE_HZ)]
    sample_rate: u32,

    #[arg(long, default_value_t = 0)]
    channel: u32,

    #[arg(long, default_value_t = 1.0)]
    volume: f32,

    #[arg(long)]
    agent_identity: Option<String>,

    #[arg(long)]
    benchmark: bool,

    #[arg(long, default_value_t = -42.0)]
    user_speech_threshold_dbfs: f32,

    #[arg(long, default_value_t = 250)]
    user_silence_hold_ms: u64,

    #[arg(long, default_value_t = -38.0)]
    speaker_speech_threshold_dbfs: f32,

    #[arg(long, default_value_t = 300)]
    speaker_confirm_ms: u64,
}

struct DedicatedRuntime {
    name: &'static str,
    handle: tokio::runtime::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl DedicatedRuntime {
    fn new(name: &'static str) -> Result<Self> {
        let (handle_tx, handle_rx) = std::sync::mpsc::sync_channel(1);
        let thread =
            thread::Builder::new().name(format!("agent-audio-{name}")).spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build dedicated tokio runtime");
                let handle = runtime.handle().clone();
                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                handle_tx
                    .send((handle, shutdown_tx))
                    .expect("failed to send runtime handle to main thread");
                runtime.block_on(async move {
                    let _ = shutdown_rx.await;
                });
            })?;

        let (handle, shutdown_tx) = handle_rx
            .recv()
            .map_err(|_| anyhow!("failed to receive dedicated runtime handle for {name}"))?;

        Ok(Self { name, handle, shutdown_tx: Some(shutdown_tx), thread: Some(thread) })
    }

    fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(future)
    }

    fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(thread) = self.thread.take() {
            if let Err(err) = thread.join() {
                info!("dedicated runtime thread '{}' exited with error: {:?}", self.name, err);
            }
        }
    }
}

impl Drop for DedicatedRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    if args.list_devices {
        return list_audio_devices();
    }
    if !(0.0..=1.0).contains(&args.volume) {
        return Err(anyhow!("volume must be between 0.0 and 1.0"));
    }

    let url = args
        .url
        .clone()
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .ok_or_else(|| anyhow!("missing LiveKit URL, set --url or LIVEKIT_URL"))?;
    let token = resolve_token(&args)?;

    let host = cpal::default_host();
    let input_device = select_input_device(&host, args.input_device.as_deref())?;
    let output_device = select_output_device(&host, args.output_device.as_deref())?;
    let input_supported_config = input_device.default_input_config()?;
    let output_supported_config = output_device.default_output_config()?;

    /*
    if args.sample_rate != SAMPLE_RATE_HZ {
        return Err(anyhow!(
            "this example is tuned for {} Hz real-time audio; got {}",
            SAMPLE_RATE_HZ,
            args.sample_rate
        ));
    }
    */

    let available_channels = input_supported_config.channels() as u32;
    if args.channel >= available_channels {
        return Err(anyhow!(
            "input channel {} is out of range for device with {} channels",
            args.channel,
            available_channels
        ));
    }

    let input_name = input_device.name().unwrap_or_else(|_| "unknown".to_string());
    let output_name = output_device.name().unwrap_or_else(|_| "unknown".to_string());
    info!("input device: {input_name}");
    info!("output device: {output_name}");

    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("connected to room: {} ({})", room.name(), room.sid().await);

    let latency_bench = Arc::new(Mutex::new(TurnLatencyBench::new(TurnLatencyBenchConfig {
        enabled: args.benchmark,
        user_speech_threshold_dbfs: args.user_speech_threshold_dbfs,
        user_silence_hold: Duration::from_millis(args.user_silence_hold_ms),
        speaker_speech_threshold_dbfs: args.speaker_speech_threshold_dbfs,
        speaker_confirm_duration: Duration::from_millis(args.speaker_confirm_ms),
        ..Default::default()
    })));

    if latency_bench.lock().unwrap().enabled() {
        info!(
            "turn benchmark enabled: user_threshold={}dBFS user_silence={}ms speaker_threshold={}dBFS speaker_confirm={}ms",
            args.user_speech_threshold_dbfs,
            args.user_silence_hold_ms,
            args.speaker_speech_threshold_dbfs,
            args.speaker_confirm_ms
        );
    }

    let livekit_source = NativeAudioSource::new(
        AudioSourceOptions {
            echo_cancellation: false,
            noise_suppression: false,
            auto_gain_control: false,
        },
        args.sample_rate,
        1,
        // Fast path for real-time audio: disable the SDK-side queue and push exact 10 ms frames
        // directly into WebRTC. The uplink loop below already slices microphone data to 10 ms.
        0,
    );

    let local_track = LocalAudioTrack::create_audio_track(
        "microphone",
        RtcAudioSource::Native(livekit_source.clone()),
    );

    room.local_participant()
        .publish_track(
            LocalTrack::Audio(local_track),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await?;
    info!("published local audio track");

    let mixer = AudioMixer::new(args.sample_rate, 1, args.volume);
    let (audio_tx, audio_rx) = mpsc::unbounded_channel();

    // Real-time audio is sensitive to scheduler stalls. The microphone uplink path and
    // the room downlink/playback path each get a dedicated Tokio runtime thread so that
    // control-plane work or remote-audio processing on one side does not delay the other.
    let mut uplink_runtime = DedicatedRuntime::new("uplink")?;
    let mut downlink_runtime = DedicatedRuntime::new("downlink")?;

    let cpal_buffer_frames_10ms: u32 = args.sample_rate / 100;
    info!("about to start audio capture on device '{}': sample rate: {} Hz, {} channels, channel {}, StreamConfig(channels: {}, sample_rate: {}, buffer_size: Fixed({} frames), format: {}, num_channels: {}",
        input_name,
        args.sample_rate, available_channels, args.channel, input_supported_config.channels(),
        args.sample_rate, cpal_buffer_frames_10ms, input_supported_config.sample_format(),
        input_supported_config.channels());
    let _capture = AudioCapture::new(
        input_device,
        StreamConfig {
            channels: input_supported_config.channels(),
            sample_rate: SampleRate(args.sample_rate),
            // Request a 10 ms device buffer to keep capture latency predictable.
            buffer_size: cpal::BufferSize::Fixed(cpal_buffer_frames_10ms),
        },
        input_supported_config.sample_format(),
        audio_tx,
        args.channel,
        available_channels,
        if args.benchmark { Some(latency_bench.clone()) } else { None },
    )?;

    info!("about to start audio playback");
    let _playback = AudioPlayback::new(
        output_device,
        StreamConfig {
            channels: 1,
            sample_rate: SampleRate(args.sample_rate),
            // Match speaker output to the same 10 ms pacing used by capture and WebRTC.
            buffer_size: cpal::BufferSize::Fixed(cpal_buffer_frames_10ms),
        },
        output_supported_config.sample_format(),
        mixer.clone(),
        if args.benchmark { Some(latency_bench.clone()) } else { None },
    )?;

    info!("starting uplink to LiveKit task");
    let uplink_task =
        uplink_runtime.spawn(stream_audio_to_livekit(audio_rx, livekit_source, args.sample_rate));

    info!("starting remote audio handling task");
    let remote_audio_task = downlink_runtime.spawn(handle_remote_audio(
        room.clone(),
        mixer,
        args.sample_rate,
        args.agent_identity.clone(),
    ));

    info!("audio app is running, press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;
    info!("shutting down");

    uplink_task.abort();
    remote_audio_task.abort();
    room.close().await?;
    uplink_runtime.shutdown();
    downlink_runtime.shutdown();
    Ok(())
}

async fn stream_audio_to_livekit(
    mut audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
    livekit_source: NativeAudioSource,
    sample_rate: u32,
) -> Result<()> {
    let mut buffer = Vec::new();
    let samples_per_10ms = (sample_rate / 100) as usize;

    while let Some(audio_data) = audio_rx.recv().await {
        buffer.extend_from_slice(&audio_data);

        while buffer.len() >= samples_per_10ms {
            let chunk: Vec<i16> = buffer.drain(..samples_per_10ms).collect();

            let frame = AudioFrame {
                data: chunk.into(),
                sample_rate,
                num_channels: 1,
                samples_per_channel: samples_per_10ms as u32,
            };

            livekit_source.capture_frame(&frame).await?;
        }
    }

    Ok(())
}

async fn handle_remote_audio(
    room: Arc<Room>,
    mixer: AudioMixer,
    sample_rate: u32,
    agent_identity: Option<String>,
) -> Result<()> {
    let mut room_events = room.subscribe();

    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::ParticipantConnected(participant) => {
                info!("participant connected: {}", participant.identity());
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                info!("participant disconnected: {}", participant.identity());
            }
            RoomEvent::TrackSubscribed { track, participant, .. } => {
                if let Some(expected) = agent_identity.as_deref() {
                    if participant.identity().to_string() != expected {
                        debug!(
                            "ignoring remote audio from non-agent participant {}",
                            participant.identity()
                        );
                        continue;
                    }
                }

                if let RemoteTrack::Audio(audio_track) = track {
                    let identity = participant.identity().to_string();
                    info!("subscribed to remote audio track from {identity}");
                    let mixer = mixer.clone();

                    tokio::spawn(async move {
                        let mut stream =
                            NativeAudioStream::new(audio_track.rtc_track(), sample_rate as i32, 1);

                        while let Some(frame) = stream.next().await {
                            mixer.add_audio_data(frame.data.as_ref());
                        }

                        info!("remote audio stream ended for {identity}");
                    });
                }
            }
            other => {
                debug!("room event: {other:?}");
            }
        }
    }

    Ok(())
}

fn resolve_token(args: &Args) -> Result<String> {
    if let Some(token) = args.token.clone().or_else(|| env::var("LIVEKIT_TOKEN").ok()) {
        return Ok(token);
    }

    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .ok_or_else(|| anyhow!("missing token and API key; set --token or --api-key"))?;
    let api_secret = args
        .api_secret
        .clone()
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .ok_or_else(|| anyhow!("missing token and API secret; set --token or --api-secret"))?;

    let room_name: String = args.room_name.clone() + "-" + &generate_random_string(8);
    info!("connnecting to room: {}", room_name);
    Ok(access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name,
            ..Default::default()
        })
        .to_jwt()?)
}

fn list_audio_devices() -> Result<()> {
    let host = cpal::default_host();
    println!("Input devices:");
    for device in host.input_devices()? {
        let name = device.name().unwrap_or_else(|_| "unknown".to_string());
        println!("  {name}");
    }
    println!("Output devices:");
    for device in host.output_devices()? {
        let name = device.name().unwrap_or_else(|_| "unknown".to_string());
        println!("  {name}");
    }
    Ok(())
}

fn select_input_device(host: &cpal::Host, requested_name: Option<&str>) -> Result<Device> {
    if let Some(name) = requested_name {
        return find_device(host.input_devices()?, name, "input");
    }
    host.default_input_device().ok_or_else(|| anyhow!("no default input device available"))
}

fn select_output_device(host: &cpal::Host, requested_name: Option<&str>) -> Result<Device> {
    if let Some(name) = requested_name {
        return find_device(host.output_devices()?, name, "output");
    }
    host.default_output_device().ok_or_else(|| anyhow!("no default output device available"))
}

fn find_device(devices: impl Iterator<Item = Device>, needle: &str, kind: &str) -> Result<Device> {
    for device in devices {
        if let Ok(name) = device.name() {
            if name.contains(needle) {
                return Ok(device);
            }
        }
    }
    Err(anyhow!("{kind} device containing '{needle}' was not found"))
}
