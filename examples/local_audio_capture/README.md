# Local Audio Capture Example

This example demonstrates how to capture audio from a local microphone and stream it to a LiveKit room while simultaneously playing back audio from other participants. It provides a complete bidirectional audio experience with real-time level monitoring.

## Features

- **Bidirectional Audio**: Capture from local microphone and play back remote participants
- **Device Selection**: Choose specific input/output devices or use system defaults
- **Real-time Level Meter**: Visual dB meter showing local microphone levels
- **Audio Processing**: Echo cancellation, noise suppression, and auto gain control (enabled by default)
- **Volume Control**: Adjustable playback volume for remote participants
- **Audio Mixing**: Combines audio from multiple remote participants
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

1. **Set Environment Variables**:

```bash
export LIVEKIT_URL="wss://your-livekit-server.com"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
```

2. **Build the Example**:

```bash
cd examples/local_audio_capture
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

Stream audio with default settings:

```bash
cargo run
```

Join a specific room with custom identity:

```bash
cargo run -- --room-name "my-meeting" --identity "john-doe"
```

### Advanced Configuration

```bash
cargo run -- \
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
cargo run -- --no-playback
```

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
| `--identity <NAME>` | LiveKit participant identity | "audio-streamer" |
| `--room-name <NAME>` | LiveKit room name | "audio-room" |

## Features in Detail

### Real-time Audio Level Meter

The example displays a real-time dB meter showing your microphone input levels:

```
Local Audio Level
────────────────────────────────────────
Mic Level: -12.3 dB [██████████████████████████▓▓▓▓▓▓▓▓▓▓▓▓▓▓]
```

- **Green bars (█)**: Normal levels
- **Yellow bars (▓)**: High levels
- **Red bars (▒)**: Very high levels (potential clipping)

### Audio Processing

The WebRTC audio processing pipeline includes:

- **Echo Cancellation**: Removes acoustic feedback between microphone and speakers
- **Noise Suppression**: Reduces background noise
- **Auto Gain Control**: Automatically adjusts microphone sensitivity

All processing features are enabled by default for optimal audio quality.

### Bidirectional Audio

The example handles both directions:

1. **Outgoing**: Captures from your microphone → processes → streams to LiveKit
2. **Incoming**: Receives audio from other participants → mixes → plays through speakers

## Architecture

### Components

1. **AudioCapture**: Captures audio from input devices using `cpal`
2. **AudioMixer**: Combines audio streams from multiple remote participants
3. **AudioPlayback**: Plays mixed audio through output devices
4. **LiveKit Integration**: Handles room connection and audio streaming

### Data Flow

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐    ┌──────────────┐
│ Microphone  │───▶│ AudioCapture │───▶│ Processing  │───▶│ LiveKit Room │
└─────────────┘    └──────────────┘    └─────────────┘    └──────────────┘
                                                                    │
┌─────────────┐    ┌──────────────┐    ┌─────────────┐              │
│ Speakers    │◀───│ AudioPlayback│◀───│ AudioMixer  │◀─────────────┘
└─────────────┘    └──────────────┘    └─────────────┘
```

## Troubleshooting

### Common Issues

1. **"No default input device available"**
   - Check microphone connection and system audio settings
   - List devices with `--list-devices` to see available options

2. **"Permission denied"**
   - **macOS**: Grant microphone permissions in System Preferences
   - **Linux**: Add user to `audio` group: `sudo usermod -a -G audio $USER`
   - **Windows**: Check privacy settings for microphone access

3. **"Device not found"**
   - Use exact device names from `--list-devices` output
   - Device names are case-sensitive

4. **Audio feedback/echo**
   - Use headphones instead of speakers
   - Ensure echo cancellation is enabled (default)
   - Reduce volume with `--volume` option

5. **Poor audio quality**
   - Try different sample rates (44100, 48000)
   - Check microphone levels in system settings
   - Ensure stable network connection

6. **High latency**
   - Use lower sample rates if needed
   - Check system audio buffer settings
   - Ensure adequate CPU resources

### Debug Information

Enable detailed logging:

```bash
RUST_LOG=debug cargo run
```

## Example Scenarios

### Online Meeting

```bash
cargo run -- \
  --room-name "team-standup" \
  --identity "alice" \
  --input-device "USB Headset" \
  --output-device "USB Headset" \
  --volume 0.9
```

### Podcast Recording

```bash
cargo run -- \
  --room-name "podcast-session" \
  --identity "host" \
  --input-device "Audio Interface" \
  --sample-rate 48000 \
  --channels 2 \
  --volume 0.7
```

### Live Streaming

```bash
cargo run -- \
  --room-name "live-stream" \
  --identity "streamer" \
  --input-device "Studio Microphone" \
  --no-playback
```

## Integration Notes

This example can be combined with other LiveKit features:

- **Video Streaming**: Add video tracks alongside audio
- **Screen Sharing**: Share screen content with audio
- **Recording**: Record the audio session
- **Multiple Participants**: Handle rooms with many participants

## Performance Considerations

- **CPU Usage**: Audio processing features increase CPU load
- **Memory**: Audio buffers scale with participant count
- **Network**: Higher sample rates increase bandwidth usage
- **Latency**: Balance between audio quality and real-time performance

## License

This example is part of the LiveKit Rust SDK under the Apache 2.0 license. 