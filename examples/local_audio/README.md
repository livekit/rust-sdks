# Local Audio Capture Example

This example demonstrates how to capture audio from a local microphone and stream it to a LiveKit room while simultaneously playing back audio from other participants. It also includes a dedicated subscriber binary that can subscribe to a single participant's audio track and report end-to-end latency from packet-trailer user timestamps, plus a single-process loopback benchmark that creates both a publisher and subscriber in one script.

## Features

- **Bidirectional Audio**: Capture from local microphone and play back remote participants
- **Device Selection**: Choose specific input/output devices or use system defaults
- **Real-time Level Meter**: Visual dB meter showing local microphone levels
- **Audio Processing**: Echo cancellation, noise suppression, and auto gain control (enabled by default)
- **Volume Control**: Adjustable playback volume for remote participants
- **Audio Mixing**: Combines audio from multiple remote participants
- **Packet-Trailer Timestamps**: Optional `PTF_USER_TIMESTAMP` support for publish-side latency measurement
- **Latency Subscriber**: Dedicated subscriber binary for measuring receive latency from a specific participant
- **Single-Process Loopback Benchmark**: Launch publisher and subscriber identities together to measure end-to-end mic latency with timestamps and frame IDs
- **Format Support**: Handles F32, I16, and U16 sample formats
- **Cross-platform**: Works on Windows, macOS, and Linux

## Prerequisites

1. **Rust**: Install Rust 1.70+ from [rustup.rs](https://rustup.rs/)
2. **LiveKit Server**: Access to a LiveKit server instance
3. **Audio Devices**: Working microphone and speakers/headphones
4. **System Permissions**: Audio device access permissions

### Platform-specific Requirements

- **macOS**: Grant microphone permissions in System Preferences → Privacy & Security → Microphone
- **Windows**: Ensure audio drivers are installed and microphone is not in use by other applications
- **Linux**: May need ALSA or PulseAudio libraries (`sudo apt install libasound2-dev` on Ubuntu/Debian)

## Setup

1. **LiveKit Connection Details** (choose one method):

   **Option A: Environment Variables**
   ```bash
   export LIVEKIT_URL="wss://your-livekit-server.com"
   export LIVEKIT_API_KEY="your-api-key"
   export LIVEKIT_API_SECRET="your-api-secret"
   ```

   **Option B: CLI Arguments**  
   Pass connection details directly to the command (see examples below)

   **Note**: CLI arguments take precedence over environment variables. You can mix both methods - for example, set API credentials via environment variables but override the URL via CLI.

2. **Build the Example**:

```bash
cd examples/local_audio
cargo build --release
```

## Usage

### List Available Audio Devices

```bash
cargo run -- --list-devices
```

Example output:
```
Available Input Devices:
───────────────────────────────────────────────────────────────
1. MacBook Pro Microphone
   ├─ Sample Rate: 8000-48000 Hz
   ├─ Channels: 1-2
   └─ Formats: F32, I16

2. USB Microphone
   ├─ Sample Rate: 44100-48000 Hz
   ├─ Channels: 1-2
   └─ Formats: F32, I16

Default Input Device: MacBook Pro Microphone

Available Output Devices:
───────────────────────────────────────────────────────────────
1. MacBook Pro Speakers
   ├─ Sample Rate: 8000-48000 Hz
   ├─ Channels: 2
   └─ Formats: F32, I16

2. USB Headphones
   ├─ Sample Rate: 44100-48000 Hz
   ├─ Channels: 2
   └─ Formats: F32, I16

Default Output Device: MacBook Pro Speakers
```

### Basic Usage

Stream audio with default settings (using environment variables):

```bash
cargo run
```

Measure latency from a specific publisher:

```bash
cargo run --bin subscriber -- \
  --participant "rust-audio-streamer"
```

Using CLI arguments for connection details:

```bash
cargo run -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret"
```

Join a specific room with custom identity:

```bash
cargo run -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --room-name "my-meeting" \
  --identity "john-doe"
```

### Advanced Configuration

```bash
cargo run -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --input-device "USB Microphone" \
  --output-device "USB Headphones" \
  --sample-rate 44100 \
  --channels 2 \
  --volume 0.8 \
  --room-name "conference-room"
```

### Capture-Only Mode

Disable audio playback and only capture:

```bash
cargo run -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --no-playback
```

### Enable Packet-Trailer User Timestamps

Publish microphone audio with `PTF_USER_TIMESTAMP` enabled so the subscriber binary can measure latency:

```bash
cargo run -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --attach-timestamp
```

Then run the subscriber against that participant identity:

```bash
cargo run --bin subscriber -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --room-name "audio-room" \
  --participant "rust-audio-streamer"
```

### Run The Single-Process Loopback Benchmark

Create both a publisher and subscriber in one process and measure microphone-to-subscriber latency:

```bash
cargo run --bin latency_loopback -- \
  --url "wss://your-project.livekit.cloud" \
  --api-key "your-api-key" \
  --api-secret "your-api-secret" \
  --input-device "USB Microphone" \
  --room-name "audio-room"
```

This benchmark publishes 10 ms microphone frames with both `user_timestamp` and `frame_id` attached, then logs latency stats and frame ID gaps from the subscriber side.
The publish timestamp is derived from the captured input chunk time and advanced in 10 ms steps, so it is suitable for relative latency benchmarking rather than exact hardware capture timing.

## Command Line Options

| Option | Description | Default |
|--------|-------------|---------|
| `--list-devices` | List available audio devices and exit | - |
| `--input-device <NAME>` | Input device name | System default |
| `--output-device <NAME>` | Output device name | System default |
| `--sample-rate <HZ>` | Sample rate in Hz | 48000 |
| `--channels <COUNT>` | Number of channels | 1 |
| `--echo-cancellation` | Enable echo cancellation | true |
| `--noise-suppression` | Enable noise suppression | true |
| `--auto-gain-control` | Enable auto gain control | true |
| `--no-playback` | Disable audio playback (capture only) | false |
| `--volume <LEVEL>` | Playback volume (0.0 to 1.0) | 1.0 |
| `--identity <NAME>` | LiveKit participant identity | "rust-audio-streamer" |
| `--room-name <NAME>` | LiveKit room name | "audio-room" |
| `--url <URL>` | LiveKit server URL | From LIVEKIT_URL env var |
| `--api-key <KEY>` | LiveKit API key | From LIVEKIT_API_KEY env var |
| `--api-secret <SECRET>` | LiveKit API secret | From LIVEKIT_API_SECRET env var |

## Loopback Benchmark Binary

Use the `latency_loopback` binary when you want to create both the publisher and subscriber in the same script.

| Option | Description | Default |
|--------|-------------|---------|
| `--list-devices` | List available audio input devices and exit | - |
| `--input-device <NAME>` | Input device name | System default |
| `--sample-rate <HZ>` | Capture and publish sample rate | 48000 |
| `--channel <INDEX>` | Input channel index to capture | 0 |
| `--room-name <NAME>` | LiveKit room name | "audio-room" |
| `--publisher-identity <NAME>` | Publisher participant identity | "rust-audio-latency-publisher" |
| `--subscriber-identity <NAME>` | Subscriber participant identity | "rust-audio-latency-subscriber" |
| `--track-name <NAME>` | Published audio track name | "latency-microphone" |
| `--url <URL>` | LiveKit server URL | From LIVEKIT_URL env var |
| `--api-key <KEY>` | LiveKit API key | From LIVEKIT_API_KEY env var |
| `--api-secret <SECRET>` | LiveKit API secret | From LIVEKIT_API_SECRET env var |

## Subscriber Binary

Use the `subscriber` binary to subscribe only to a specific participant's first audio track and log latency from the attached user timestamp.

| Option | Description | Default |
|--------|-------------|---------|
| `--identity <NAME>` | Subscriber participant identity | "rust-audio-latency-subscriber" |
| `--participant <NAME>` | Target remote participant identity to subscribe to | Required |
| `--room-name <NAME>` | LiveKit room name | "audio-room" |
| `--sample-rate <HZ>` | Output sample rate for the decoded PCM stream | 48000 |
| `--channels <COUNT>` | Output channel count for the decoded PCM stream | 1 |
| `--url <URL>` | LiveKit server URL | From LIVEKIT_URL env var |
| `--api-key <KEY>` | LiveKit API key | From LIVEKIT_API_KEY env var |
| `--api-secret <SECRET>` | LiveKit API secret | From LIVEKIT_API_SECRET env var |
