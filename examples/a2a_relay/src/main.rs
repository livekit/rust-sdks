// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use anyhow::{anyhow, Result};
use clap::Parser;
use futures_util::StreamExt;
use log::{info, warn};
use std::env;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, Duration};

use livekit::options::TrackPublishOptions;
use livekit::track::TrackSource;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::{AudioSourceOptions, RtcAudioSource};
use livekit::{prelude::LocalAudioTrack, Room, RoomEvent, RoomOptions};
use livekit_a2a_relay::{A2aClient, A2aFrame, EnergyVad, RelayActor};
use livekit_api::access_token;

mod local_onnx;

/// CLI Arguments for the A2A Relay Example
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "LiveKit A2A Relay — bridges a LiveKit room with an A2A-compliant agent",
    long_about = None,
)]
struct Args {
    /// LiveKit server URL (overrides LIVEKIT_URL env var)
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (overrides LIVEKIT_API_KEY env var)
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret (overrides LIVEKIT_API_SECRET env var)
    #[arg(long)]
    api_secret: Option<String>,

    /// LiveKit participant identity
    #[arg(long, default_value = "a2a-relay-agent")]
    identity: String,

    /// LiveKit room name to join
    #[arg(long, default_value = "a2a-room")]
    room_name: String,

    /// Sample rate in Hz (default: 16000)
    #[arg(short, long, default_value_t = 16000)]
    sample_rate: u32,

    /// Base URL of the A2A-compliant agent (e.g. https://my-agent.example.com).
    ///
    /// When provided the relay connects to the real agent using the official
    /// A2A Rust SDK instead of the built-in mock client.
    ///
    /// The agent must advertise a `JSONRPC` or `HTTP+JSON` interface in its
    /// agent card at `<agent-url>/.well-known/agent.json`.
    #[arg(long)]
    agent_url: Option<String>,

    /// Run with fully local ONNX speech recognition (Whisper STT) and synthesis (Piper TTS)
    #[arg(long, default_value_t = false)]
    local_onnx: bool,

    /// Directory containing the Whisper and Piper ONNX models
    #[arg(long, default_value = "./models")]
    model_dir: String,

    /// Voice Activity Detection (VAD) energy threshold. Lower values are more sensitive.
    #[arg(long, default_value_t = 0.015)]
    vad_threshold: f32,

    /// STT Model quality (tiny, base, small). Requires corresponding downloaded models.
    #[arg(long, default_value = "tiny")]
    stt_model: String,

    /// TTS Model quality (medium, high). Requires corresponding downloaded models.
    #[arg(long, default_value = "medium")]
    tts_model: String,
}

// ---------------------------------------------------------------------------
// Mock A2A client (used when --agent-url is not provided)
// ---------------------------------------------------------------------------

/// A stub [`A2aClient`] that logs incoming audio and periodically emits
/// synthetic TTS frames so the relay pipeline can be exercised without a real
/// agent.
struct MockA2aClient {
    _frame_tx: mpsc::UnboundedSender<A2aFrame>,
    frame_rx: Mutex<Option<mpsc::UnboundedReceiver<A2aFrame>>>,
}

impl MockA2aClient {
    fn new() -> (Self, mpsc::UnboundedSender<A2aFrame>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let client = Self { _frame_tx: tx.clone(), frame_rx: Mutex::new(Some(rx)) };
        (client, tx)
    }
}

impl A2aClient for MockA2aClient {
    fn send_audio(
        &self,
        turn_id: u64,
        samples: &[i16],
    ) -> impl Future<Output = Result<(), String>> + Send {
        let len = samples.len();
        async move {
            info!(
                "MockA2aClient: received {} audio samples from LiveKit (turn={}, not forwarded)",
                len, turn_id
            );
            Ok(())
        }
    }

    fn cancel_turn(&self, turn_id: u64) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            warn!("MockA2aClient: turn {} cancelled", turn_id);
            Ok(())
        }
    }

    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            info!("MockA2aClient: floor requested");
            Ok(())
        }
    }

    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            info!("MockA2aClient: floor released");
            Ok(())
        }
    }

    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame> {
        self.frame_rx.lock().expect("lock poisoned").take().expect("subscribe_frames called twice")
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    let args = Args::parse();

    // Resolve LiveKit connection parameters.
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .ok_or_else(|| anyhow!("LIVEKIT_URL must be provided via --url or env var"))?;
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .ok_or_else(|| anyhow!("LIVEKIT_API_KEY must be provided via --api-key or env var"))?;
    let api_secret =
        args.api_secret.or_else(|| env::var("LIVEKIT_API_SECRET").ok()).ok_or_else(|| {
            anyhow!("LIVEKIT_API_SECRET must be provided via --api-secret or env var")
        })?;

    // Mint a LiveKit access token.
    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            ..Default::default()
        })
        .to_jwt()?;

    // Connect to the LiveKit room.
    info!("Connecting to LiveKit room '{}'…", args.room_name);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, mut events_rx) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected to room: {}", room.name());

    // Create the local audio source that carries agent TTS back to the room.
    let audio_options = AudioSourceOptions {
        echo_cancellation: false,
        noise_suppression: false,
        auto_gain_control: false,
    };
    let audio_source = NativeAudioSource::new(audio_options, args.sample_rate, 1, 1000);
    let track = LocalAudioTrack::create_audio_track(
        "agent-audio",
        RtcAudioSource::Native(audio_source.clone()),
    );
    room.local_participant()
        .publish_track(
            livekit::prelude::LocalTrack::Audio(track.clone()),
            TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() },
        )
        .await?;
    info!("Agent audio track published");

    // Build VAD and relay actor shutdown channel.
    let vad_detector = EnergyVad::new(args.vad_threshold, 200, args.sample_rate);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (subscriber_audio_tx, subscriber_audio_rx) = mpsc::unbounded_channel();

    // Decide which A2A client backend to use.
    if let Some(ref agent_url) = args.agent_url {
        if args.local_onnx {
            info!("Running in local ONNX mode (Whisper STT + Piper TTS) connecting to text agent at '{}'…", agent_url);
            let a2a_client = local_onnx::LocalOnnxA2aClient::new(
                agent_url.clone(),
                &args.model_dir,
                args.vad_threshold,
                &args.stt_model,
                &args.tts_model,
            );
            let a2a_client = Arc::new(a2a_client);

            let relay_actor = RelayActor::new(
                room.clone(),
                a2a_client,
                track,
                audio_source,
                vad_detector,
                args.sample_rate,
                1,
            );

            run_relay(
                relay_actor,
                room,
                events_rx,
                shutdown_tx,
                shutdown_rx,
                subscriber_audio_tx,
                subscriber_audio_rx,
            )
            .await?;
        } else {
            // ── Real agent via the official A2A Rust SDK ────────────────────────
            info!("Connecting to A2A agent at '{}'…", agent_url);

            #[cfg(feature = "a2a-client")]
            {
                let a2a_client = livekit_a2a_relay::OfficialA2aClient::from_agent_url(agent_url)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let a2a_client = Arc::new(a2a_client);

                let relay_actor = RelayActor::new(
                    room.clone(),
                    a2a_client,
                    track,
                    audio_source,
                    vad_detector,
                    args.sample_rate,
                    1,
                );

                run_relay(
                    relay_actor,
                    room,
                    events_rx,
                    shutdown_tx,
                    shutdown_rx,
                    subscriber_audio_tx,
                    subscriber_audio_rx,
                )
                .await?;
            }

            #[cfg(not(feature = "a2a-client"))]
            {
                anyhow::bail!(
                    "--agent-url requires this binary to be compiled with the `a2a-client` feature.\n\
                     Re-run with: cargo run -F a2a-client --example a2a_relay -- --agent-url {agent_url}"
                );
            }
        }
    } else {
        // ── Mock client with synthetic TTS ──────────────────────────────────
        info!("No --agent-url provided; using mock A2A client with synthetic TTS");

        let (a2a_client, simulated_tts_tx) = MockA2aClient::new();
        let a2a_client = Arc::new(a2a_client);
        let relay_turn_manager = {
            let relay_actor = RelayActor::new(
                room.clone(),
                a2a_client,
                track,
                audio_source,
                vad_detector,
                args.sample_rate,
                1,
            );
            let tm = relay_actor.turn_manager();

            let (shutdown_rx_inner, subscriber_audio_rx_inner) = (shutdown_rx, subscriber_audio_rx);

            tokio::spawn(relay_actor.run(shutdown_rx_inner, subscriber_audio_rx_inner));
            tm
        };

        // Subscribe to remote participant tracks.
        spawn_track_subscriber(room.clone(), args.sample_rate, subscriber_audio_tx);

        // Periodically emit synthetic TTS frames.
        let simulated_tts_task = tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(8)).await;
                info!("Simulating A2A TTS speaking start…");
                let current_turn = relay_turn_manager.current_turn();
                for _ in 0..20 {
                    let samples = vec![5000i16; 1600]; // 100 ms @ 16 kHz
                    let frame = A2aFrame { turn_id: current_turn, samples };
                    if simulated_tts_tx.send(frame).is_err() {
                        break;
                    }
                    sleep(Duration::from_millis(100)).await;
                }
                info!("Simulating A2A TTS speaking end.");
            }
        });

        info!("A2A Relay (mock) running. Press Ctrl+C to stop.");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down…");
            }
            _ = async {
                while let Some(event) = events_rx.recv().await {
                    if let RoomEvent::Disconnected { reason } = event {
                        warn!("Disconnected from room: {:?}", reason);
                        break;
                    }
                }
            } => {}
        }

        simulated_tts_task.abort();
        let _ = shutdown_tx.send(true);
        let _ = room.close().await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run the relay actor loop and handle Ctrl-C / room disconnection.
#[allow(clippy::too_many_arguments)]
async fn run_relay<C, V>(
    relay_actor: RelayActor<C, V>,
    room: Arc<Room>,
    mut events_rx: mpsc::UnboundedReceiver<RoomEvent>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    subscriber_audio_tx: mpsc::UnboundedSender<Vec<i16>>,
    subscriber_audio_rx: mpsc::UnboundedReceiver<Vec<i16>>,
) -> Result<()>
where
    C: A2aClient,
    V: livekit_a2a_relay::VadDetector,
{
    spawn_track_subscriber(room.clone(), 16000, subscriber_audio_tx);
    let relay_handle = tokio::spawn(relay_actor.run(shutdown_rx, subscriber_audio_rx));

    info!("A2A Relay running. Press Ctrl+C to stop.");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl-C received, shutting down…");
        }
        _ = async {
            loop {
                match events_rx.recv().await {
                    Some(RoomEvent::Disconnected { reason }) => {
                        warn!("Disconnected from room: {:?}", reason);
                        break;
                    }
                    Some(_) => {}
                    None => break, // channel closed
                }
            }
        } => {}
    }

    let _ = shutdown_tx.send(true);
    let _ = relay_handle.await;
    let _ = room.close().await;
    Ok(())
}

/// Spawn a background task that subscribes to every remote audio track and
/// forwards PCM samples to `audio_tx`.
fn spawn_track_subscriber(
    room: Arc<Room>,
    sample_rate: u32,
    audio_tx: mpsc::UnboundedSender<Vec<i16>>,
) {
    // 1. Process any participants/tracks already present and subscribed in the room
    use futures_util::StreamExt;
    for (_, participant) in room.remote_participants() {
        for (_, publication) in participant.track_publications() {
            if let Some(track) = publication.track() {
                if let livekit::track::RemoteTrack::Audio(audio_track) = track {
                    info!(
                        "Subscribed to existing audio track from participant '{}'",
                        participant.identity()
                    );
                    let mut audio_stream =
                        NativeAudioStream::new(audio_track.rtc_track(), sample_rate as i32, 1);
                    let tx = audio_tx.clone();
                    tokio::spawn(async move {
                        while let Some(frame) = audio_stream.next().await {
                            let _ = tx.send(frame.data.to_vec());
                        }
                    });
                }
            }
        }
    }

    // 2. Process new subscriptions
    tokio::spawn(async move {
        let mut room_events = room.subscribe();
        while let Some(event) = room_events.recv().await {
            if let RoomEvent::TrackSubscribed { track, participant, .. } = event {
                if let livekit::track::RemoteTrack::Audio(audio_track) = track {
                    info!(
                        "Subscribed to audio track from participant '{}'",
                        participant.identity()
                    );
                    let mut audio_stream =
                        NativeAudioStream::new(audio_track.rtc_track(), sample_rate as i32, 1);
                    let tx = audio_tx.clone();
                    tokio::spawn(async move {
                        while let Some(frame) = audio_stream.next().await {
                            let _ = tx.send(frame.data.to_vec());
                        }
                    });
                }
            }
        }
    });
}
