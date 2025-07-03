# Local Audio Capture Example

This example demonstrates how to capture audio from local microphone devices and stream it to a LiveKit room using the LiveKit Rust SDK.

## Features

- **Device Enumeration**: List all available audio input devices
- **Flexible Device Selection**: Choose specific microphone or use system default
- **Audio Processing**: Optional echo cancellation, noise suppression, and auto gain control
- **Real-time Streaming**: Low-latency audio capture and streaming to LiveKit
- **Configurable Parameters**: Adjust sample rate, channels, and audio processing settings
- **Robust Error Handling**: Graceful handling of device access and streaming errors

## Prerequisites

1. **Rust**: Install Rust 1.70+ from [rustup.rs](https://rustup.rs/)
2. **LiveKit Server**: Access to a LiveKit server instance
3. **Audio Device**: A working microphone or audio input device
4. **System Audio**: Appropriate permissions for audio device access

### Platform-specific Requirements

- **macOS**: May require microphone permissions in System Preferences
- **Windows**: Ensure audio drivers are properly installed
- **Linux**: May need ALSA or PulseAudio development libraries

## Setup

1. **Environment Variables**: Set the required LiveKit connection details:

```bash
export LIVEKIT_URL="wss://your-livekit-server.com"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
```

2. **Build the Example**:

```bash
cd examples/local_audio_capture
cargo build
```

## Usage

### List Available Audio Devices

Before streaming, you can list all available audio input devices:

```bash
cargo run -- --list-devices
```

This will output something like:
```
Available audio input devices:
─────────────────────────────
1. MacBook Pro Microphone
   └─ Sample rate: 48000 Hz
   └─ Channels: 1
   └─ Sample format: F32

2. USB Microphone
   └─ Sample rate: 44100 Hz
   └─ Channels: 2
   └─ Sample format: I16

Default device: MacBook Pro Microphone
```

### Basic Audio Streaming

Stream audio using the default microphone:

```bash
cargo run
```

### Advanced Configuration

Use a specific device with custom settings:

```bash
cargo run -- \
  --device "USB Microphone" \
  --sample-rate 44100 \
  --channels 2 \
  --echo-cancellation \
  --noise-suppression \
  --auto-gain-control
```

## Command Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--list-devices` | `-l` | List available audio devices and exit | - |
| `--device <NAME>` | `-d` | Specify audio device by name | System default |
| `--sample-rate <HZ>` | `-s` | Audio sample rate in Hz | 48000 |
| `--channels <COUNT>` | `-c` | Number of audio channels | 1 |
| `--echo-cancellation` | - | Enable echo cancellation | false |
| `--noise-suppression` | - | Enable noise suppression | false |
| `--auto-gain-control` | - | Enable automatic gain control | false |

## Architecture

### Components

1. **Audio Capture (`AudioCapture`)**
   - Uses `cpal` library for cross-platform audio device access
   - Supports multiple sample formats (F32, I16, U16)
   - Handles format conversion to 16-bit PCM for LiveKit

2. **Buffer Management**
   - Collects audio samples from the device callback
   - Buffers data into 10ms chunks for optimal LiveKit streaming
   - Manages sample rate and channel configuration

3. **LiveKit Integration**
   - Creates `NativeAudioSource` with configurable audio processing
   - Publishes audio track as microphone source
   - Handles room connection and participant management

### Data Flow

```
Microphone → cpal → Format Conversion → Buffer → LiveKit AudioFrame → LiveKit Room
```

## Audio Processing Options

The example supports WebRTC's built-in audio processing features:

- **Echo Cancellation**: Removes acoustic echo from the audio signal
- **Noise Suppression**: Reduces background noise
- **Auto Gain Control**: Automatically adjusts microphone gain levels

Enable these features using command-line flags:

```bash
cargo run -- --echo-cancellation --noise-suppression --auto-gain-control
```

## Troubleshooting

### Common Issues

1. **"No default input device available"**
   - Check that a microphone is connected and recognized by the system
   - Try listing devices with `--list-devices` to see available options

2. **"Permission denied" or access errors**
   - On macOS: Grant microphone permissions in System Preferences → Security & Privacy
   - On Linux: Ensure user is in the `audio` group
   - On Windows: Check that the microphone is not being used by another application

3. **"Device not found"**
   - Use `--list-devices` to see exact device names
   - Device names are case-sensitive and must match exactly

4. **Audio quality issues**
   - Try different sample rates (44100, 48000)
   - Enable audio processing options (echo cancellation, noise suppression)
   - Check microphone levels in system audio settings

5. **High latency or dropouts**
   - Reduce the buffer size in the code (currently 1000ms)
   - Check system audio latency settings
   - Ensure stable network connection to LiveKit server

### Debug Logging

Enable detailed logging to troubleshoot issues:

```bash
RUST_LOG=debug cargo run
```

## Configuration Examples

### High-Quality Studio Microphone

```bash
cargo run -- \
  --device "Blue Yeti" \
  --sample-rate 48000 \
  --channels 2 \
  --auto-gain-control
```

### Laptop Built-in Microphone with Noise Reduction

```bash
cargo run -- \
  --sample-rate 48000 \
  --channels 1 \
  --echo-cancellation \
  --noise-suppression \
  --auto-gain-control
```

### Low-Latency Gaming Setup

```bash
cargo run -- \
  --device "Gaming Headset" \
  --sample-rate 44100 \
  --channels 1
```

## Integration with Other Examples

This example can be used alongside other LiveKit examples:

1. **With Video**: Combine with camera capture for full audio/video streaming
2. **With Recording**: Use `save_to_disk` example to record the audio stream
3. **With Processing**: Add custom audio effects before streaming

## Performance Considerations

- **CPU Usage**: Audio processing features increase CPU usage
- **Memory**: Larger buffer sizes use more memory but may reduce dropouts
- **Network**: Higher sample rates and channel counts increase bandwidth usage
- **Latency**: Smaller buffer sizes reduce latency but may cause audio glitches

## Contributing

To extend this example:

1. **Add Audio Effects**: Implement custom audio processing in the streaming pipeline
2. **Multiple Devices**: Support streaming from multiple microphones simultaneously  
3. **Auto Device Selection**: Implement smart device selection based on quality metrics
4. **Dynamic Configuration**: Allow runtime changes to audio settings
5. **Monitoring**: Add audio level meters and quality monitoring

## License

This example is part of the LiveKit Rust SDK and follows the same license terms. 