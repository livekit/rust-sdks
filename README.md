# LiveKit A2A Relay (`livekit-a2a-relay`)

An ultra-low latency bridge that seamlessly connects a LiveKit WebRTC room to any Agent-to-Agent (A2A) compliant HTTP/SSE endpoint.

This crate abstracts away the complexities of real-time audio synchronization, allowing you to focus on building intelligent AI agents while LiveKit handles the high-performance WebRTC transport.

## Features

- 🎭 **Actor-Based Concurrency**: Runs on a dedicated Tokio thread isolated from WebRTC FFI native threads to prevent audio stuttering or deadlocks.
- ⚡ **Ultra-Low Latency**: Built-in non-blocking `AudioJitterBuffer` absorbs network jitter from HTTP SSE streams.
- 🗣️ **Intelligent Turn Management**: Floor requests, cancellations, and stale frame invalidation are handled automatically.
- 🎙️ **Voice Activity Detection (VAD)**: Includes an `EnergyVad` implementation for immediate speech detection, easily extensible to other VAD algorithms.
- 🌐 **Official A2A Client**: (Optional) Built-in implementation to connect to standard A2A endpoints out of the box.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
livekit-a2a-relay = { version = "0.1.0", features = ["a2a-client"] }
```

### Feature Flags
- `a2a-client`: Enables the `OfficialA2aClient`, providing a production-ready HTTP/SSE client for interacting with A2A agents. Requires `reqwest` and cryptographic dependencies.

## Build Requirements

Because `webrtc-sys` compiles native WebRTC C++ code and relies on code-generation via `bindgen`, the following system dependencies are required:

1. **`cmake`**: Must be installed and accessible in the system `PATH`.
2. **Clang / LLVM Headers**: Ensure Clang has access to system C headers. If compilation of cryptographic or WebRTC sys-crates (such as `aws-lc-sys`) fails due to missing `stddef.h`, set the `BINDGEN_EXTRA_CLANG_ARGS` environment variable to point to the Clang include directory. For example, on Linux systems with LLVM-20:
   ```bash
   export BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/llvm-20/lib/clang/20/include"
   ```

## Architecture

The system operates via four primary components:

1. **`RelayActor`**: The core event loop. You spawn this on a Tokio task. It routes audio between the user (via LiveKit `NativeAudioSource`) and the Agent.
2. **`VadDetector`**: Analyzes raw PCM audio in real-time. When speech is detected, it signals the `RelayActor` to interrupt the agent and take the floor.
3. **`TurnManager`**: Tracks conversation "turns" (IDs) to ensure that if a user interrupts an agent, any delayed audio frames arriving from the network belonging to the old turn are safely dropped.
4. **`A2aClient`**: The trait defining how the relay communicates with the agent backend. 

## Usage Example

Here is a simplified example of how to bridge a LiveKit room to an A2A agent:

```rust
use livekit_a2a_relay::{RelayActor, OfficialA2aClient, EnergyVad};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    // 1. Connect to LiveKit and set up your audio sources
    // let (room, audio_source, local_track) = ... 

    // 2. Initialize the official A2A HTTP client
    let agent_url = "https://your-agent-endpoint.com";
    let a2a_client = OfficialA2aClient::from_agent_url(agent_url).await.unwrap();
    let a2a_client = Arc::new(a2a_client);

    // 3. Initialize Voice Activity Detection
    let sample_rate = 16000;
    let vad = EnergyVad::new(0.015, 200, sample_rate);

    // 4. Create the Relay Actor
    let relay = RelayActor::new(
        room,
        a2a_client,
        local_track,
        audio_source,
        vad,
        sample_rate,
        1, // channels
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (audio_in_tx, audio_in_rx) = mpsc::unbounded_channel();

    // Subscribe incoming room audio to `audio_in_tx` here...

    // 5. Run the Relay loop
    tokio::spawn(relay.run(shutdown_rx, audio_in_rx));

    // The relay is now managing bidirectional audio and A2A floor states!
}
```

## Running the Examples

This repository includes fully functional examples to help you test the pipeline locally. 

### Local STT/TTS (ONNX)
Run the relay with entirely local STT (Whisper) and TTS (Piper) models, connecting to a local mock text agent:

```bash
# Terminal 1: Run the mock agent
cargo run -p a2a_mock_agent -- --port 8000

# Terminal 2: Run the relay in local-onnx mode
cargo run -p a2a_relay_example -- \
  --url http://127.0.0.1:7880 \
  --api-key devkey --api-secret secret \
  --room-name test-room \
  --agent-url http://127.0.0.1:8000 \
  --local-onnx --model-dir ./models
```
*(Requires running `./scripts/download_onnx_models.sh` first to fetch the models)*.
