<<<<<<< HEAD
# LiveKit A2A Relay (`livekit-a2a-relay`)

An ultra-low latency bridge that seamlessly connects a LiveKit WebRTC room to any Agent-to-Agent (A2A) compliant HTTP/SSE endpoint.

This crate abstracts away the complexities of real-time audio synchronization, allowing you to focus on building intelligent AI agents while LiveKit handles the high-performance WebRTC transport.

---

## Package Information

| Parameter | Value |
| :--- | :--- |
| **Crate Name** | `livekit-a2a-relay` |
| **Version** | `0.1.0` |
| **Rust Edition** | `2021` |
| **Minimum Supported Rust Version (MSRV)** | `1.75+` |
| **License** | Apache-2.0 |
| **Repository** | [github.com/livekit/rust-sdks](https://github.com/livekit/rust-sdks) |

### Feature Flags

- **`default`**: Minimal feature set, building only the core actor, jitter buffer, and VAD systems.
- **`a2a-client`**: Enables the `OfficialA2aClient` backed by the upstream `a2a-client` crate. This provides an out-of-the-box, production-grade HTTP/SSE client for interacting with standard A2A agents. *Pulls in additional cryptographic and networking dependencies (`reqwest`, `uuid`).*
- **`a2a-integration`**: Enables both the `a2a-client` and the SLIM-RPC transport (experimental/work-in-progress).

---

## Build Requirements

Because `webrtc-sys` compiles native WebRTC C++ code and relies on code-generation via `bindgen`, the following system dependencies are required:

1. **`cmake`**: Must be installed and accessible in the system `PATH`.
2. **Clang / LLVM Headers**: Ensure Clang has access to system C headers. If compilation of cryptographic or WebRTC sys-crates (such as `aws-lc-sys`) fails due to missing `stddef.h`, set the `BINDGEN_EXTRA_CLANG_ARGS` environment variable to point to the Clang include directory. For example, on Linux systems with LLVM-20:
   ```bash
   export BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/llvm-20/lib/clang/20/include"
   ```

---

## Architecture & Data Flow

The relay actor coordinates bidirectional audio streams on separate execution contexts to guarantee stutter-free playback and fast interruption responsiveness.

### Bidirectional Flow Diagram

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant LiveKit as LiveKit WebRTC Track
    participant Relay as RelayActor Loop
    participant Jitter as AudioJitterBuffer
    participant A2A as A2A Client / Agent

    Note over User, LiveKit: User Speaking Flow
    User->>LiveKit: Sends Mic Audio (PCM)
    LiveKit->>Relay: Unbounded Channel (PCM chunks)
    Note right of Relay: Run Voice Activity Detection (VAD)
    alt Speech Detected (Interruption)
        Relay->>Relay: Next conversational Turn ID
        Relay->>LiveKit: Clear output playback buffers
        Relay->>Jitter: Clear queue
        Relay->>A2A: cancel_turn(prev_turn)
        Relay->>A2A: request_floor()
    end
    Relay->>A2A: send_audio(turn_id, PCM)

    Note over A2A, LiveKit: Agent Response Flow
    A2A-->>Relay: subscribe_frames() stream
    alt Turn ID matches Current Turn
        Relay->>Jitter: push(samples)
    else Turn ID is Stale
        Relay->>Relay: Discard frame (Stale Interrupted Turn)
    end
    
    loop Every 10ms (Playback Tick)
        Relay->>Jitter: pop(samples_per_frame)
        Note right of Jitter: Returns comfort noise if underflow
        Relay->>LiveKit: capture_frame() to NativeAudioSource
    end
```

### Core Architecture Components

1. **`RelayActor`**: The coordinator running the main event loop. It bridges LiveKit's real-time WebRTC track callback/channels with A2A client network events.
2. **`TurnManager`**: An atomic turn counter ensuring synchronization. When a user interrupts, a new turn is generated; all incoming agent audio matching previous turns is discarded.
3. **`AudioJitterBuffer`**: A ring buffer that smooths playback against clock-drift and network jitter. On underflow, comfort noise is inserted; on overflow, old frames are dropped.
4. **`EnergyVad`**: A Voice Activity Detector measuring root-mean-square (RMS) energy. Features a configurable hangover window to prevent word truncation.

---

## API Reference

### 1. `RelayActor<C, V>`
The core struct that drives the conversational loop.
- **`new`**: Initializes the actor with a LiveKit room, an `A2aClient` implementation, audio tracks, VAD model, sample rate, and channel count.
- **`run`**: Consumes the actor and runs the async event loop handling shutdown watch signals and subscriber audio channels.

### 2. `A2aClient` Trait
Implement this trait to plug in a custom A2A transport layer.
```rust
pub trait A2aClient: Send + Sync + 'static {
    /// Send a buffer of PCM samples to the remote agent for the given turn.
    fn send_audio(&self, turn_id: u64, samples: &[i16]) -> impl Future<Output = Result<(), String>> + Send;

    /// Cancel the specified turn (due to user interruption).
    fn cancel_turn(&self, turn_id: u64) -> impl Future<Output = Result<(), String>> + Send;

    /// Request the speaking floor from the remote agent.
    fn request_floor(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// Release the speaking floor.
    fn release_floor(&self) -> impl Future<Output = Result<(), String>> + Send;

    /// Subscribe to incoming frames from the agent.
    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame>;
}
```

---

## Detailed Usage Guides

### Example 1: Spawning a Relay with the Official Client
This guide shows how to initialize a LiveKit WebRTC connection and spin up the relay utilizing the `OfficialA2aClient` (backed by the standard HTTP/SSE endpoint protocol).

```rust
use livekit_a2a_relay::{RelayActor, OfficialA2aClient, EnergyVad};
use livekit::prelude::*;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Connect to a LiveKit Room
    let url = "http://localhost:7880";
    let token = "YOUR_LIVEKIT_TOKEN";
    let (room, mut room_events) = Room::connect(url, token, RoomOptions::default()).await?;
    let room = Arc::new(room);

    // 2. Setup your NativeAudioSource for sending playback audio to LiveKit
    let sample_rate = 16000;
    let num_channels = 1;
    let audio_source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        sample_rate,
        num_channels,
    );
    let track = LocalAudioTrack::create_audio_track(
        "agent-voice",
        audio_source.clone().into(),
    );
    room.local_participant().publish_track(track.clone(), TrackPublishOptions::default()).await?;

    // 3. Initialize the Official A2A HTTP client
    let agent_url = "https://my-a2a-agent.example.com";
    let a2a_client = Arc::new(OfficialA2aClient::from_agent_url(agent_url).await?);

    // 4. Initialize the Voice Activity Detector
    let rms_threshold = 0.015; // RMS energy threshold
    let hangover_ms = 250;      // Hold active state 250ms after speech ends
    let vad = EnergyVad::new(rms_threshold, hangover_ms, sample_rate);

    // 5. Create and run the Relay Actor
    let relay = RelayActor::new(
        room.clone(),
        a2a_client,
        track,
        audio_source,
        vad,
        sample_rate,
        num_channels,
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (audio_in_tx, audio_in_rx) = mpsc::unbounded_channel();

    // 6. Spawn the actor in the background
    tokio::spawn(relay.run(shutdown_rx, audio_in_rx));

    // 7. Route incoming user/participant audio to the relay channel
    tokio::spawn(async move {
        while let Some(event) = room_events.recv().await {
            if let RoomEvent::TrackSubscribed { track, .. } = event {
                if let RemoteTrack::Audio(audio_track) = track {
                    let mut reader = audio_track.get_reader();
                    let audio_in_tx = audio_in_tx.clone();
                    tokio::spawn(async move {
                        while let Some(frame) = reader.next().await {
                            // Extract 16-bit signed PCM from frame and send to relay
                            let samples: Vec<i16> = frame.data.as_ref().to_vec();
                            let _ = audio_in_tx.send(samples);
                        }
                    });
                }
            }
        }
    });

    // Wait or listen for shutdown conditions...
=======
<!--BEGIN_BANNER_IMAGE-->

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="/.github/banner_dark.png">
  <source media="(prefers-color-scheme: light)" srcset="/.github/banner_light.png">
  <img style="width:100%;" alt="The LiveKit icon, the name of the repository and some sample code in the background." src="https://raw.githubusercontent.com/livekit/rust-sdks/main/.github/banner_light.png">
</picture>

<!--END_BANNER_IMAGE-->

# 📹🎙️🦀 Rust Client SDK for LiveKit

<!--BEGIN_DESCRIPTION-->
Use this SDK to add realtime video, audio and data features to your Rust app. By connecting to <a href="https://livekit.io/">LiveKit</a> Cloud or a self-hosted server, you can quickly build applications such as multi-modal AI, live streaming, or video calls with just a few lines of code.
<!--END_DESCRIPTION-->

[![crates.io](https://img.shields.io/crates/v/livekit.svg)](https://crates.io/crates/livekit)
[![livekit docs.rs](https://img.shields.io/docsrs/livekit)](https://docs.rs/livekit/latest/)
[![Builds](https://github.com/livekit/rust-sdks/actions/workflows/builds.yml/badge.svg?branch=main)](https://github.com/livekit/rust-sdks/actions/workflows/builds.yml)
[![Tests](https://github.com/livekit/rust-sdks/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/livekit/rust-sdks/actions/workflows/tests.yml)

## Features

- [x] Receiving tracks
- [x] Publishing tracks
- [x] Data channels
- [x] Simulcast
- [x] SVC codecs (AV1/VP9)
- [ ] Adaptive Streaming
- [x] Dynacast
- [x] Hardware video enc/dec
  - [x] H.264, H.265 using VideoToolbox (MacOS/iOS)
  - [x] H.264, H.265 on NVidia discrete GPUs (Linux)
  - [x] H.264, H.265 on AMD CPUs & GPUs (Linux)
  - [x] H.264, H.265, AV1 on NVidia Jetson (Linux)
- Supported Platforms
  - [x] Windows
  - [x] MacOS
  - [x] Linux
  - [x] iOS
  - [x] Android

## Crates

- `livekit-api`: Server APIs and auth token generation
- `livekit`: LiveKit real-time SDK
- `livekit-ffi`: Internal crate, used to generate bindings for other languages
- `livekit-protocol`: LiveKit protocol generated code

When adding the SDK as a dependency to your project, make sure to add the
[necessary `rustflags`](https://github.com/livekit/rust-sdks/blob/main/.cargo/config.toml)
to your cargo config, otherwise linking may fail.

Also, please refer to the list of the [supported platform toolkits](https://github.com/livekit/rust-sdks/blob/main/.github/workflows/builds.yml).

## Getting started

Currently, Tokio is required to use this SDK, however we plan to make the async executor runtime agnostic.

## Using Server API

### Generating an access token

```rust
use livekit_api::access_token;
use std::env;

fn create_token() -> Result<String, access_token::AccessTokenError> {
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-bot")
        .with_name("Rust Bot")
        .with_grants(access_token::VideoGrants {
             room_join: true,
             room: "my-room".to_string(),
             ..Default::default()
        })
        .to_jwt();
    return token
}
```

### Creating a room with RoomService API

```rust
use livekit_api::services::room::{CreateRoomOptions, RoomClient};

#[tokio::main]
async fn main() {
    let room_service = RoomClient::new("http://localhost:7880").unwrap();

    let room = room_service
        .create_room("my_room", CreateRoomOptions::default())
        .await
        .unwrap();

    println!("Created room: {:?}", room);
}
```

## Using Real-time SDK

### Connect to a Room and listen for events:

```rust
use livekit::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let (room, mut room_events) = Room::connect(&url, &token).await?;

    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                // ...
            }
            _ => {}
        }
    }

>>>>>>> ef0b36a8472308828310d7eb646a93752cb0a821
    Ok(())
}
```

<<<<<<< HEAD
### Example 2: Implementing a Custom A2A Client
If your agent uses a custom WebRTC, WebSockets, or gRPC protocol instead of HTTP/SSE, you can implement the `A2aClient` trait directly.

```rust
use livekit_a2a_relay::{A2aClient, A2aFrame};
use tokio::sync::mpsc;

pub struct CustomA2aClient {
    // Custom WebSocket connection or gRPC channel
    frame_tx: mpsc::UnboundedSender<A2aFrame>,
}

impl A2aClient for CustomA2aClient {
    async fn send_audio(&self, turn_id: u64, samples: &[i16]) -> Result<(), String> {
        // Send audio PCM chunks over WebSockets / custom protocol
        Ok(())
    }

    async fn cancel_turn(&self, turn_id: u64) -> Result<(), String> {
        // Send interruption signal to agent
        Ok(())
    }

    async fn request_floor(&self) -> Result<(), String> {
        Ok(())
    }

    async fn release_floor(&self) -> Result<(), String> {
        Ok(())
    }

    fn subscribe_frames(&self) -> mpsc::UnboundedReceiver<A2aFrame> {
        // Return a channel receiver where you push incoming frames generated by the agent
        let (_tx, rx) = mpsc::unbounded_channel();
        rx
    }
}
```

---

## Running the Examples & Tests

### 1. Integration Tests
Run the mock server integration tests locally:
```bash
BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/llvm-20/lib/clang/20/include" \
PATH="/home/jayaprakash/Android/Sdk/cmake/3.22.1/bin:$PATH" \
cargo test --features a2a-client
```

### 2. Local STT/TTS (ONNX) Example
Run the relay with entirely local Whisper (STT) and Piper (TTS) models:

```bash
# Terminal 1: Run the mock agent
cargo run -p a2a_mock_agent -- --port 8000

# Terminal 2: Download the models
./scripts/download_onnx_models.sh

# Terminal 3: Run the local ONNX example pipeline
cargo run -p a2a_relay_example -- \
  --url http://127.0.0.1:7880 \
  --api-key devkey --api-secret secret \
  --room-name test-room \
  --agent-url http://127.0.0.1:8000 \
  --local-onnx --model-dir ./models
```
=======
### Receive video frames of a subscribed track

```rust
...
use futures::StreamExt; // this trait is required for iterating on audio & video frames
use livekit::prelude::*;

match event {
    RoomEvent::TrackSubscribed { track, publication, participant } => {
        match track {
            RemoteTrack::Audio(audio_track) => {
                let rtc_track = audio_track.rtc_track();
                let mut audio_stream = NativeAudioStream::new(rtc_track);
                tokio::spawn(async move {
                    // Receive the audio frames in a new task
                    while let Some(audio_frame) = audio_stream.next().await {
                        log::info!("received audio frame - {audio_frame:#?}");
                    }
                });
            },
            RemoteTrack::Video(video_track) => {
                let rtc_track = video_track.rtc_track();
                let mut video_stream = NativeVideoStream::new(rtc_track);
                tokio::spawn(async move {
                    // Receive the video frames in a new task
                    while let Some(video_frame) = video_stream.next().await {
                        log::info!("received video frame - {video_frame:#?}");
                    }
                });
            },
        }
    },
    _ => {}
}
```

## Examples

![](https://github.com/livekit/rust-sdks/blob/main/examples/images/simple-room-demo.gif)

- [basic room](https://github.com/livekit/rust-sdks/tree/main/examples/basic_room): simple example connecting to a room.
- [wgpu_room](https://github.com/livekit/rust-sdks/tree/main/examples/wgpu_room): complete example app with video rendering using wgpu and egui.
- [mobile](https://github.com/livekit/rust-sdks/tree/main/examples/mobile): mobile app targeting iOS and Android
- [play_from_disk](https://github.com/livekit/rust-sdks/tree/main/examples/play_from_disk): publish audio from a wav file
- [save_to_disk](https://github.com/livekit/rust-sdks/tree/main/examples/save_to_disk): save received audio to a wav file

## Building

### MacOS

When building on MacOS, `-ObjC` linker flag is needed. LiveKit's WebRTC implementation make use of ObjectiveC libraries on the Mac. You may get the following error if the app isn't linked with ObjC:

```
*** Terminating app due to uncaught exception 'NSInvalidArgumentException', reason: '-[RTCVideoCodecInfo nativeSdpVideoFormat]: unrecognized selector sent to instance 0x600003bc6660'
```

## Motivation and Design Goals

LiveKit aims to provide an open source, end-to-end WebRTC stack that works everywhere. We have two goals in mind with this SDK:

1. Build a standalone, cross-platform LiveKit client SDK for Rustaceans.
2. Build a common core for other platform-specific SDKs (e.g. Unity, Unreal, iOS, Android)

Regarding (2), we've already developed a number of [client SDKs](https://github.com/livekit?q=client-sdk&type=all) for several platforms and encountered a few challenges in the process:

- There's a significant amount of business/control logic in our signaling protocol and WebRTC. Currently, this logic needs to be implemented in every new platform we support.
- Interactions with media devices and encoding/decoding are specific to each platform and framework.
- For multi-platform frameworks (e.g. Unity, Flutter, React Native), the aforementioned tasks proved to be extremely painful.

Thus, we posited a Rust SDK, something we wanted build anyway, encapsulating all our business logic and platform-specific APIs into a clean set of abstractions, could also serve as the foundation for our other SDKs!

We'll first use it as a basis for our Unity SDK (under development), but over time, it will power our other SDKs, as well.

<!--BEGIN_REPO_NAV-->
<br/><table>
<thead><tr><th colspan="2">LiveKit Ecosystem</th></tr></thead>
<tbody>
<tr><td>Agents SDKs</td><td><a href="https://github.com/livekit/agents">Python</a> · <a href="https://github.com/livekit/agents-js">Node.js</a></td></tr><tr></tr>
<tr><td>LiveKit SDKs</td><td><a href="https://github.com/livekit/client-sdk-js">Browser</a> · <a href="https://github.com/livekit/client-sdk-swift">Swift</a> · <a href="https://github.com/livekit/client-sdk-android">Android</a> · <a href="https://github.com/livekit/client-sdk-flutter">Flutter</a> · <a href="https://github.com/livekit/client-sdk-react-native">React Native</a> · <b>Rust</b> · <a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <a href="https://github.com/livekit/client-sdk-unity">Unity</a> · <a href="https://github.com/livekit/client-sdk-unity-web">Unity (WebGL)</a> · <a href="https://github.com/livekit/client-sdk-esp32">ESP32</a> · <a href="https://github.com/livekit/client-sdk-cpp">C++</a></td></tr><tr></tr>
<tr><td>Starter Apps</td><td><a href="https://github.com/livekit-examples/agent-starter-python">Python Agent</a> · <a href="https://github.com/livekit-examples/agent-starter-node">TypeScript Agent</a> · <a href="https://github.com/livekit-examples/agent-starter-react">React App</a> · <a href="https://github.com/livekit-examples/agent-starter-swift">SwiftUI App</a> · <a href="https://github.com/livekit-examples/agent-starter-android">Android App</a> · <a href="https://github.com/livekit-examples/agent-starter-flutter">Flutter App</a> · <a href="https://github.com/livekit-examples/agent-starter-react-native">React Native App</a> · <a href="https://github.com/livekit-examples/agent-starter-embed">Web Embed</a></td></tr><tr></tr>
<tr><td>UI Components</td><td><a href="https://github.com/livekit/components-js">React</a> · <a href="https://github.com/livekit/components-android">Android Compose</a> · <a href="https://github.com/livekit/components-swift">SwiftUI</a> · <a href="https://github.com/livekit/components-flutter">Flutter</a></td></tr><tr></tr>
<tr><td>Server APIs</td><td><a href="https://github.com/livekit/node-sdks">Node.js</a> · <a href="https://github.com/livekit/server-sdk-go">Golang</a> · <a href="https://github.com/livekit/server-sdk-ruby">Ruby</a> · <a href="https://github.com/livekit/server-sdk-kotlin">Java/Kotlin</a> · <a href="https://github.com/livekit/python-sdks">Python</a> · <b>Rust</b> · <a href="https://github.com/agence104/livekit-server-sdk-php">PHP (community)</a> · <a href="https://github.com/pabloFuente/livekit-server-sdk-dotnet">.NET (community)</a></td></tr><tr></tr>
<tr><td>Resources</td><td><a href="https://docs.livekit.io">Docs</a> · <a href="https://docs.livekit.io/mcp">Docs MCP Server</a> · <a href="https://github.com/livekit/livekit-cli">CLI</a> · <a href="https://cloud.livekit.io">LiveKit Cloud</a></td></tr><tr></tr>
<tr><td>LiveKit Server OSS</td><td><a href="https://github.com/livekit/livekit">LiveKit server</a> · <a href="https://github.com/livekit/egress">Egress</a> · <a href="https://github.com/livekit/ingress">Ingress</a> · <a href="https://github.com/livekit/sip">SIP</a></td></tr><tr></tr>
<tr><td>Community</td><td><a href="https://community.livekit.io">Developer Community</a> · <a href="https://livekit.io/join-slack">Slack</a> · <a href="https://x.com/livekit">X</a> · <a href="https://www.youtube.com/@livekit_io">YouTube</a></td></tr>
</tbody>
</table>
<!--END_REPO_NAV-->
>>>>>>> ef0b36a8472308828310d7eb646a93752cb0a821
