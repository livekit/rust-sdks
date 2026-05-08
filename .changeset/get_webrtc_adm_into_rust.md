---
libwebrtc: patch
livekit: patch
webrtc-sys: patch
---

Get WebRTC ADM into Rust - #1037 (@xianshijing-lk)

This PR introduces platform audio device management via WebRTC's Audio Device Module (ADM).

### Features
- **ADM Proxy**: New `AdmProxy` class that switches between Dummy ADM (synthetic mode) and Platform ADM (real audio I/O)
- **PlatformAudio API**: High-level Rust API for microphone capture and speaker playout with AEC/AGC/NS
- **Device enumeration**: List and select recording/playout devices by index or GUID
- **Mode switching**: Seamlessly switch between synthetic mode (FFI callbacks) and platform mode (native speakers) while audio is active
- **Audio processing**: Configure echo cancellation, noise suppression, and auto gain control with platform-specific defaults (hardware on iOS, software elsewhere)

### Audio Modes
| Mode | Recording | Playout | Use Case |
|------|-----------|---------|----------|
| Synthetic | NativeAudioSource | Dummy ADM + FFI | Unity audio, agents |
| Platform | Platform ADM mic | Platform ADM speakers | VoIP with AEC |

### API
```rust
// Create PlatformAudio for microphone/speaker access
let audio = PlatformAudio::new()?;

// Enumerate and select devices
for i in 0..audio.recording_devices() as u16 {
    println!("Mic {}: {}", i, audio.recording_device_name(i));
}
audio.set_recording_device(0)?;

// Create audio track for publishing
let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
```
