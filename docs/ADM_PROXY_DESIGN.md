# Audio Device Module (ADM) Proxy Design Document

## Overview

This document describes the design and implementation of the Audio Device Module (ADM) Proxy system in the LiveKit Rust SDK. The ADM Proxy enables platform audio device access while maintaining backward compatibility with manual audio pushing via `NativeAudioSource`.

---

## Goals

### Problem Statement

WebRTC's `AudioDeviceModule` (ADM) is traditionally configured at `PeerConnectionFactory` creation time. The SDK needs to support two audio capture methods:

1. **Manual audio push** (default): Applications push audio frames via `NativeAudioSource`
2. **Platform audio capture**: WebRTC captures from the system microphone automatically

These two methods must coexist without interference.

### Design Goals

| Goal | Description |
|------|-------------|
| **Dual Audio Support** | Support both `NativeAudioSource` (manual push) and platform microphone capture |
| **Multiple Audio Tracks** | Allow multiple audio tracks with different sources simultaneously |
| **Backward Compatible** | Existing code using `NativeAudioSource` continues to work unchanged |
| **Clean Public API** | Expose a simple `PlatformAudio` interface for device management |
| **FFI Support** | Platform audio available for FFI clients (Python, Unity, etc.) |

### Non-Goals / Limitations

| Limitation | Description |
|------------|-------------|
| **Index-based device IDs** | Device indices may change on hot-plug |
| **Process-global** | Audio configuration is process-global, not per-room |

---

## Architecture

### High-Level Design

The SDK uses a **recording gate** pattern rather than mode switching:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Audio Architecture                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ PeerConnectionFactory (created once at startup)                      │   │
│  │  └─ AdmProxy (wraps Platform ADM)                                    │   │
│  │      ├─ Platform ADM: Always created and initialized                 │   │
│  │      └─ recording_enabled_: Gate for microphone access (default OFF) │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                           │                                                 │
│         ┌─────────────────┼─────────────────────┐                          │
│         ▼                 ▼                     ▼                          │
│   ┌──────────────┐  ┌──────────────┐    ┌──────────────┐                   │
│   │ Device Track │  │ Native Track │    │ Native Track │                   │
│   │ (Microphone) │  │   (TTS)      │    │ (Screen Cap) │                   │
│   └──────────────┘  └──────────────┘    └──────────────┘                   │
│         │                 │                     │                          │
│         │                 │                     │                          │
│  Uses AudioState    Uses AddSink          Uses AddSink                     │
│  (is_external=false) (is_external=true)   (is_external=true)               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Components

1. **AdmProxy**: Wraps WebRTC's platform ADM with a recording gate
2. **PlatformAudio**: Rust API for enabling platform audio and device management
3. **NativeAudioSource**: Existing API for manual audio frame pushing
4. **external_audio_source.patch**: WebRTC patch to prevent audio mixing conflicts

### Recording Gate Pattern

Instead of swapping ADM implementations, the SDK uses a simple boolean gate:

```cpp
// adm_proxy.h
class AdmProxy : public webrtc::AudioDeviceModule {
  // Platform ADM is ALWAYS created at startup
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_;

  // Gate controls whether microphone recording is active
  // Default: FALSE - NativeAudioSource works without interference
  bool recording_enabled_ = false;
};
```

When `recording_enabled_ = false`:
- `InitRecording()` returns success but does nothing
- `StartRecording()` returns success but does nothing
- Microphone is not accessed
- `NativeAudioSource` works normally

When `recording_enabled_ = true` (via `PlatformAudio::new()`):
- `InitRecording()` initializes the microphone
- `StartRecording()` starts microphone capture
- Device audio flows to tracks using `RtcAudioSource::Device`

---

## WebRTC Patching

The SDK applies a patch to WebRTC to support multiple audio sources without conflicts. This section explains the patch and why it's necessary.

### The Problem

WebRTC's `AudioState` routes device-captured audio to **all** `AudioSendStream` instances by default. This causes problems when mixing device audio with manually-pushed audio:

```
Without Patch:
  ADM (microphone) → AudioState → ALL AudioSendStreams
  NativeAudioSource → Same AudioSendStreams
  = DOUBLE FEEDING! (device audio + manual audio mixed incorrectly)
```

### The Solution: external_audio_source.patch

Located at: `webrtc-sys/libwebrtc/patches/external_audio_source.patch`

The patch adds an `is_external_source()` method to `AudioSourceInterface`:

```cpp
// api/media_stream_interface.h
class AudioSourceInterface : public MediaSourceInterface {
  // Returns true if this source delivers audio externally (via AddSink),
  // bypassing the ADM/AudioState audio distribution path.
  virtual bool is_external_source() const { return false; }
};
```

### Patch Details

**1. AudioSourceInterface addition** (`api/media_stream_interface.h`):
```cpp
// Returns true if this source delivers audio externally (via AddSink),
// bypassing the ADM/AudioState audio distribution path.
// When true, AudioSendStream should not register with AudioState.
virtual bool is_external_source() const { return false; }
```

**2. AudioSendStream::Config flag** (`call/audio_send_stream.h`):
```cpp
struct Config {
  // When true, this stream uses an external audio source (not ADM).
  // AudioState will NOT send device-captured audio to this stream.
  bool external_source = false;
};
```

**3. AudioSendStream lifecycle changes** (`audio/audio_send_stream.cc`):
```cpp
void AudioSendStream::Start() {
  // Only register with AudioState if not using external source.
  // External sources deliver audio directly via AddSink.
  if (!config_.external_source) {
    audio_state()->AddSendingStream(this, ...);
  }
}

void AudioSendStream::Stop() {
  if (!config_.external_source) {
    audio_state()->RemoveSendingStream(this);
  }
}
```

**4. Automatic detection** (`media/engine/webrtc_voice_engine.cc`):
```cpp
void WebRtcAudioSendStream::SetSource(AudioSource* source) {
  // Check if this is an external audio source
  if (source->is_external_source() && !config_.external_source) {
    config_.external_source = true;
    stream_->Reconfigure(config_, nullptr);
  }
  source->SetSink(this);
}
```

### SDK Implementation

**NativeAudioSource** (`webrtc-sys/include/livekit/audio_track.h`):
```cpp
class AudioTrackSource::InternalSource : public webrtc::LocalAudioSource {
  // Override to indicate this is an external audio source.
  // This prevents AudioState from sending device audio to streams using this source.
  bool is_external_source() const override { return true; }
};
```

**Device Source**: Uses WebRTC's built-in `LocalAudioSource` which returns `false` (default).

### Audio Flow with Patch

```
With Patch:
  ADM (microphone) → AudioState → Only streams with is_external=false (Device tracks)
  NativeAudioSource → Only streams with is_external=true (Native tracks)
  = CLEAN SEPARATION!
```

### Why Not platform_audio_source.patch?

An alternative approach would be `platform_audio_source.patch` that creates a new source type consuming from an ADM sink. This was considered but rejected:

| Approach | Pros | Cons |
|----------|------|------|
| **external_audio_source.patch** (chosen) | Minimal patch, uses standard WebRTC AudioState for device audio | Single device track per ADM |
| **platform_audio_source.patch** | Unified source model, multiple device tracks | More complex, extra buffering/latency, larger patch |

The current approach is preferred because:
1. **Minimal WebRTC modification**: Only adds a boolean flag
2. **Uses standard audio path**: Device audio uses WebRTC's battle-tested AudioState
3. **Low latency**: No extra buffering for device audio
4. **Simpler**: Less code to maintain

---

## Audio Modes

### Quick Reference

| Source Type | Use Case | Audio Flow | AEC Works? |
|-------------|----------|------------|------------|
| `RtcAudioSource::Native` | TTS, file streaming, agents | Manual push via `capture_frame()` | No |
| `RtcAudioSource::Device` | VoIP, microphone capture | Automatic via platform ADM | Yes |

### Mode 1: Manual Audio Push (Default)

Use `NativeAudioSource` to push audio frames manually. This is the default mode and requires no special setup.

```rust
use livekit::webrtc::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::RtcAudioSource;
use livekit::prelude::*;

// Create audio source for manual frame pushing
let source = NativeAudioSource::new(
    AudioSourceOptions::default(),
    48000,  // sample rate
    2,      // channels
    100,    // queue size in ms
);

// Push frames manually
source.capture_frame(&audio_frame).await;

// Create track
let track = LocalAudioTrack::create_audio_track(
    "audio",
    RtcAudioSource::Native(source)
);

// Publish the track
room.local_participant()
    .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
    .await?;
```

**Characteristics:**
- `recording_enabled_ = false` (default)
- ADM recording operations are no-ops
- Audio is pushed via `capture_frame()`
- `is_external_source() = true` prevents AudioState interference
- AEC does NOT work (no valid playout reference)

**Suitable for:**
- Server-side agents
- Text-to-speech (TTS) audio
- Audio from files or network streams
- Testing without audio hardware

---

### Mode 2: Platform Audio Capture

Use `PlatformAudio` to capture from the system microphone. WebRTC handles device management automatically.

```rust
use livekit::prelude::*;

// Create PlatformAudio instance (enables recording gate)
let audio = PlatformAudio::new()?;

// Enumerate devices
println!("Recording devices:");
for i in 0..audio.recording_devices() as u16 {
    println!("  [{}] {}", i, audio.recording_device_name(i));
}

// Select device
audio.set_recording_device(0)?;

// Connect to room
let (room, events) = Room::connect(&url, &token, RoomOptions::default()).await?;

// Create track using Device source (Platform ADM handles capture)
let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());

// Publish
room.local_participant()
    .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
    .await?;

// ... use room ...

// PlatformAudio dropped automatically when out of scope
```

**Characteristics:**
- `PlatformAudio::new()` sets `recording_enabled_ = true`
- ADM recording operations work normally
- Audio captured automatically from selected microphone
- `is_external_source() = false` allows AudioState routing
- AEC works correctly

**Platform implementations:**
| Platform | Backend |
|----------|---------|
| macOS/iOS | CoreAudio / VPIO |
| Windows | WASAPI |
| Linux | PulseAudio / ALSA |
| Android | AAudio / OpenSL ES |

---

### Combining Both Modes

You can use both `NativeAudioSource` and `PlatformAudio` simultaneously for different tracks:

```rust
use livekit::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;

// Track A: Microphone via platform audio
let mic = PlatformAudio::new()?;
let mic_track = LocalAudioTrack::create_audio_track("mic", mic.rtc_source());

// Track B: TTS via manual pushing
let tts_source = NativeAudioSource::new(opts, 48000, 2, 100);
let tts_track = LocalAudioTrack::create_audio_track(
    "tts",
    RtcAudioSource::Native(tts_source),
);

// Publish both - they don't interfere with each other
room.local_participant().publish_track(LocalTrack::Audio(mic_track), opts).await?;
room.local_participant().publish_track(LocalTrack::Audio(tts_track), opts).await?;
```

This works because:
1. `mic_track` uses `is_external_source() = false` → receives ADM audio via AudioState
2. `tts_track` uses `is_external_source() = true` → receives audio via `capture_frame()`
3. The `external_audio_source.patch` ensures they don't mix

---

## Public API

### PlatformAudio

```rust
/// Platform audio device management for microphone capture and speaker playout.
#[derive(Clone)]
pub struct PlatformAudio { ... }

impl PlatformAudio {
    /// Creates a new PlatformAudio instance.
    /// Enables ADM recording for microphone capture.
    /// Multiple instances share the same underlying ADM.
    pub fn new() -> AudioResult<Self>;

    /// Get the RTC audio source for creating tracks.
    /// Returns `RtcAudioSource::Device`.
    pub fn rtc_source(&self) -> RtcAudioSource;

    // === Device Enumeration ===

    /// Get the number of playout (speaker) devices.
    pub fn playout_devices(&self) -> i16;

    /// Get the number of recording (microphone) devices.
    pub fn recording_devices(&self) -> i16;

    /// Get the name of a playout device by index.
    pub fn playout_device_name(&self, index: u16) -> String;

    /// Get the name of a recording device by index.
    pub fn recording_device_name(&self, index: u16) -> String;

    // === Device Selection ===

    /// Set the active playout device.
    pub fn set_playout_device(&self, index: u16) -> AudioResult<()>;

    /// Set the active recording device.
    pub fn set_recording_device(&self, index: u16) -> AudioResult<()>;

    /// Switch playout device during active session (hot-swap).
    pub fn switch_playout_device(&self, index: u16) -> AudioResult<()>;

    /// Switch recording device during active session (hot-swap).
    pub fn switch_recording_device(&self, index: u16) -> AudioResult<()>;

    // === Audio Processing ===

    /// Configure audio processing (AEC, AGC, NS).
    pub fn configure_audio_processing(&self, opts: AudioProcessingOptions) -> AudioResult<()>;

    /// Enable or disable echo cancellation.
    pub fn set_echo_cancellation(&self, enabled: bool) -> AudioResult<()>;

    /// Enable or disable noise suppression.
    pub fn set_noise_suppression(&self, enabled: bool) -> AudioResult<()>;

    /// Enable or disable automatic gain control.
    pub fn set_auto_gain_control(&self, enabled: bool) -> AudioResult<()>;

    /// Explicitly release platform audio resources.
    pub fn release(self);
}
```

### AudioError

```rust
/// Errors that can occur during audio operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Platform audio could not be initialized.
    PlatformInitFailed,

    /// The specified device index is invalid.
    InvalidDeviceIndex,

    /// An audio operation failed.
    OperationFailed(String),
}

/// Result type for audio operations.
pub type AudioResult<T> = Result<T, AudioError>;
```

### RtcAudioSource

```rust
/// Audio source type for creating LocalAudioTrack.
pub enum RtcAudioSource {
    /// Manual audio push via NativeAudioSource.
    Native(NativeAudioSource),

    /// Platform device audio (microphone capture via ADM).
    Device,
}
```

---

## FFI API

The SDK provides a Protocol Buffers-based FFI interface for foreign language clients (Python, Unity, Node.js, etc.). The FFI uses a handle-based model where clients create a `PlatformAudio` handle and use it for all subsequent operations.

### Protocol Messages

Located at: `livekit-ffi/protocol/audio_manager.proto`

```protobuf
// Create a new PlatformAudio instance
message NewPlatformAudioRequest {}

message NewPlatformAudioResponse {
  oneof message {
    OwnedPlatformAudio platform_audio = 1;  // Handle on success
    string error = 2;                        // Error message on failure
  }
}

message OwnedPlatformAudio {
  FfiOwnedHandle handle = 1;
  PlatformAudioInfo info = 2;
}

message PlatformAudioInfo {
  int32 recording_device_count = 1;
  int32 playout_device_count = 2;
}

// Enumerate audio devices
message GetAudioDevicesRequest {
  uint64 platform_audio_handle = 1;
}

message GetAudioDevicesResponse {
  repeated AudioDeviceInfo playout_devices = 1;
  repeated AudioDeviceInfo recording_devices = 2;
  optional string error = 3;
}

message AudioDeviceInfo {
  uint32 index = 1;
  string name = 2;
}

// Set recording device
message SetRecordingDeviceRequest {
  uint64 platform_audio_handle = 1;
  uint32 index = 2;
}

message SetRecordingDeviceResponse {
  optional string error = 1;
}

// Set playout device
message SetPlayoutDeviceRequest {
  uint64 platform_audio_handle = 1;
  uint32 index = 2;
}

message SetPlayoutDeviceResponse {
  optional string error = 1;
}
```

### FFI Usage Pattern

**1. Create PlatformAudio Handle:**
```
Request:  NewPlatformAudioRequest {}
Response: OwnedPlatformAudio { handle: 123, info: { recording: 2, playout: 3 } }
```

**2. Enumerate Devices:**
```
Request:  GetAudioDevicesRequest { platform_audio_handle: 123 }
Response: {
  recording_devices: [
    { index: 0, name: "MacBook Pro Microphone" },
    { index: 1, name: "External USB Microphone" }
  ],
  playout_devices: [
    { index: 0, name: "MacBook Pro Speakers" },
    { index: 1, name: "External Headphones" }
  ]
}
```

**3. Select Devices:**
```
Request:  SetRecordingDeviceRequest { platform_audio_handle: 123, index: 0 }
Response: SetRecordingDeviceResponse { error: null }

Request:  SetPlayoutDeviceRequest { platform_audio_handle: 123, index: 1 }
Response: SetPlayoutDeviceResponse { error: null }
```

**4. Create Audio Track:**
Use the handle to create an audio track with `RtcAudioSource::Device`.

**5. Release Handle:**
When done, dispose the handle using `DisposeRequest`. The ADM recording is disabled when all handles are released.

### Handle Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                      FFI Client Lifecycle                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. NewPlatformAudioRequest()                                    │
│     └─→ Creates PlatformAudio, enables ADM recording             │
│     └─→ Returns handle_id (e.g., 123)                            │
│                                                                  │
│  2. GetAudioDevicesRequest(handle=123)                           │
│     └─→ Enumerates available microphones and speakers            │
│                                                                  │
│  3. SetRecordingDeviceRequest(handle=123, index=0)               │
│     └─→ Selects which microphone to use                          │
│                                                                  │
│  4. Create audio track with Device source                        │
│     └─→ Track captures from selected microphone                  │
│                                                                  │
│  5. DisposeRequest(handle=123)                                   │
│     └─→ Releases PlatformAudio, disables ADM if last handle      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Reference Counting

Multiple FFI clients can create `PlatformAudio` handles. All handles share the same underlying ADM:

```
Client A: NewPlatformAudioRequest() → handle_1 (ref_count: 1)
Client B: NewPlatformAudioRequest() → handle_2 (ref_count: 2)
Client A: DisposeRequest(handle_1)              (ref_count: 1)
Client B: DisposeRequest(handle_2)              (ref_count: 0, ADM disabled)
```

### Error Handling

FFI responses include optional error messages:

| Error | Meaning |
|-------|---------|
| `"Invalid device index"` | Device index >= device count |
| `"Platform audio initialization failed"` | ADM could not be created |
| `"Handle not found"` | Invalid or already disposed handle |

---

## Implementation Details

### AdmProxy Class

```cpp
// webrtc-sys/include/livekit/adm_proxy.h
class AdmProxy : public webrtc::AudioDeviceModule {
 public:
  explicit AdmProxy(const webrtc::Environment& env,
                    webrtc::Thread* worker_thread);
  ~AdmProxy() override;

  // Check if platform ADM was successfully initialized
  bool is_initialized() const;

  // Control whether recording (microphone) is enabled.
  // When disabled, InitRecording/StartRecording are no-ops.
  void set_recording_enabled(bool enabled);
  bool recording_enabled() const;

  // All AudioDeviceModule methods delegate to platform_adm_
  // Recording methods check recording_enabled_ first

 private:
  const webrtc::Environment& env_;
  webrtc::Thread* worker_thread_;

  // The underlying platform ADM (always created at startup)
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_;
  bool adm_initialized_ = false;

  // Recording gate - defaults to FALSE
  bool recording_enabled_ = false;
};
```

### Recording Gate Implementation

```cpp
// webrtc-sys/src/adm_proxy.cpp

int32_t AdmProxy::InitRecording() {
  if (!platform_adm_) return -1;
  if (!recording_enabled_) {
    // Return success but don't actually initialize
    return 0;
  }
  return platform_adm_->InitRecording();
}

int32_t AdmProxy::StartRecording() {
  if (!platform_adm_) return -1;
  if (!recording_enabled_) {
    // Return success but don't actually start
    return 0;
  }
  return platform_adm_->StartRecording();
}

bool AdmProxy::Recording() const {
  if (!platform_adm_) return false;
  if (!recording_enabled_) return false;
  return platform_adm_->Recording();
}
```

### PlatformAudio Reference Counting

```rust
// livekit/src/audio.rs

lazy_static! {
    static ref PLATFORM_ADM_HANDLE: Mutex<Weak<PlatformAdmHandle>> = Mutex::new(Weak::new());
}

struct PlatformAdmHandle {
    runtime: Arc<LkRuntime>,
}

impl PlatformAudio {
    pub fn new() -> AudioResult<Self> {
        let mut handle_ref = PLATFORM_ADM_HANDLE.lock();

        // Reuse existing handle if available
        if let Some(handle) = handle_ref.upgrade() {
            return Ok(Self { handle });
        }

        // Create new handle and enable recording
        let runtime = LkRuntime::instance();
        runtime.set_adm_recording_enabled(true);

        let handle = Arc::new(PlatformAdmHandle { runtime });
        *handle_ref = Arc::downgrade(&handle);

        Ok(Self { handle })
    }
}
```

---

## Backward Compatibility

### NativeAudioSource Unchanged

Existing code using `NativeAudioSource` works without any changes:

```rust
// This code works exactly as before
let source = NativeAudioSource::new(opts, 48000, 2, 100);
source.capture_frame(&frame).await;
let track = LocalAudioTrack::create_audio_track("audio", RtcAudioSource::Native(source));
```

Why it continues to work:
1. `recording_enabled_ = false` by default → ADM recording is disabled
2. `is_external_source() = true` → AudioState doesn't interfere
3. No code changes required in user applications

### Migration from AudioManager

If you previously used `AudioManager`, migrate to `PlatformAudio`:

**Before:**
```rust
let audio = AudioManager::instance();
audio.set_mode(AudioMode::Platform)?;
let track = LocalAudioTrack::create_audio_track("mic", RtcAudioSource::Device);
```

**After:**
```rust
let audio = PlatformAudio::new()?;
let track = LocalAudioTrack::create_audio_track("mic", audio.rtc_source());
```

---

## Platform-Specific Notes

### iOS

- Creates a VPIO (Voice Processing IO) AudioUnit
- Only one VPIO can exist per process
- Drop all `PlatformAudio` instances to release the microphone
- Other audio frameworks (e.g., expo-audio-studio) get silence while VPIO is active

### Android

- Hardware AEC is unreliable on many devices
- Default is software audio processing (`prefer_hardware_processing = false`)
- Use `AudioProcessingOptions` to configure

### Desktop (macOS, Windows, Linux)

- Hardware audio processing not available
- WebRTC's software APM is always used
- Device hot-plug supported via `switch_recording_device()`

---

## File Structure

```
rust-sdks/
├── webrtc-sys/
│   ├── include/livekit/
│   │   ├── adm_proxy.h              # AdmProxy class with recording gate
│   │   ├── audio_track.h            # NativeAudioSource with is_external_source()
│   │   └── peer_connection_factory.h
│   ├── src/
│   │   ├── adm_proxy.cpp            # Recording gate implementation
│   │   ├── audio_track.cpp
│   │   ├── peer_connection_factory.cpp
│   │   └── peer_connection_factory.rs  # FFI bindings
│   └── libwebrtc/
│       └── patches/
│           └── external_audio_source.patch  # WebRTC patch for multi-source support
│
├── libwebrtc/
│   └── src/
│       ├── audio_source.rs          # RtcAudioSource enum
│       └── peer_connection_factory.rs
│
├── livekit/
│   └── src/
│       ├── prelude.rs
│       ├── audio.rs                 # PlatformAudio, AudioProcessingOptions
│       └── rtc_engine/
│           └── lk_runtime.rs        # Runtime with ADM control methods
│
└── livekit-ffi/
    ├── protocol/
    │   ├── audio_manager.proto      # PlatformAudio FFI messages
    │   ├── ffi.proto                # Main FFI request/response definitions
    │   └── handle.proto             # FfiOwnedHandle message
    └── src/
        └── server/
            └── requests.rs          # FFI request handlers (FfiPlatformAudio)
```

---

## References

- [WebRTC AudioDeviceModule Documentation](https://webrtc.googlesource.com/src/+/main/modules/audio_device/g3doc/audio_device_module.md)
- [LiveKit Swift SDK - AudioManager](https://docs.livekit.io/client-sdk-swift/AudioManager/)
- [LiveKit Android SDK - AudioOptions](https://docs.livekit.io/reference/client-sdk-android/livekit-android-sdk/io.livekit.android/-audio-options/)
