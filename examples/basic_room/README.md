# Basic Room Example

Demonstrates PlatformAudio for microphone capture and WAV file publishing.

## Prerequisites

Set environment variables for LiveKit server connection:

```bash
export LIVEKIT_URL="wss://your-server.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
```

## Usage

### List audio devices

```bash
cargo run -p basic_room -- --list-devices
```

### Publish microphone audio

```bash
cargo run -p basic_room -- --platform-audio
```

### Publish WAV file

```bash
cargo run -p basic_room -- --file path/to/audio.wav
```

### Publish both microphone and WAV file

```bash
cargo run -p basic_room -- --platform-audio-and-file path/to/audio.wav
```

### Select specific devices

```bash
cargo run -p basic_room -- --platform-audio --mic-id "device-guid" --speaker-id "device-guid"
```

### Specify room name

```bash
cargo run -p basic_room -- --platform-audio --room my-custom-room
```

## All Options

```
Options:
      --list-devices                        List available audio devices and exit
      --platform-audio                      Publish microphone using PlatformAudio
      --platform-audio-and-file <WAV_PATH>  Publish both microphone and WAV file
      --file <WAV_PATH>                     Publish just WAV file (no microphone)
      --room <ROOM>                         Room name to join [default: my-room]
      --mic-id <DEVICE_ID>                  Select microphone by device ID
      --speaker-id <DEVICE_ID>              Select speaker by device ID
  -h, --help                                Print help
  -V, --version                             Print version
```
