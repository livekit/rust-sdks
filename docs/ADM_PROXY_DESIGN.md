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

### High-Level Overview

```
                         LiveKit Audio Architecture

  ┌─────────────────┐                           ┌─────────────────┐
  │   Application   │                           │   Application   │
  │  (Unity/Rust)   │                           │  (Unity/Rust)   │
  └────────┬────────┘                           └────────┬────────┘
           │                                             │
           │ capture_frame()                             │ AudioStream.next()
           ▼                                             ▲
  ┌─────────────────┐                           ┌─────────────────┐
  │NativeAudioSource│                           │NativeAudioStream│
  │(is_external=true│                           │  (FFI Callback) │
  └────────┬────────┘                           └────────┲────────┘
           │                                             ┃
           │ AddSink                                     ┃ OnData
           ▼                                             ┃
  ┌────────────────────────────────────────────────────────────────────┐
  │                         PeerConnection                              │
  │  ┌──────────────────┐                    ┌──────────────────┐      │
  │  │  AudioSendStream │                    │ AudioReceiveStream│      │
  │  │ (external=true)  │                    │                  │      │
  │  └────────┬─────────┘                    └────────┬─────────┘      │
  │           │                                       │                 │
  │           │ NOT registered                        │ decoded audio  │
  │           │ with AudioState                       │                 │
  │           ▼                                       ▼                 │
  │  ┌──────────────────────────────────────────────────────────┐      │
  │  │                      AudioState                           │      │
  │  │  (WebRTC internal - mixes/routes audio)                   │      │
  │  │                                                           │      │
  │  │  ┌───────────────────┐    ┌───────────────────────────┐  │      │
  │  │  │ SendingStreams[]  │    │ AudioMixer (for playout)  │  │      │
  │  │  │ (external=false)  │    │                           │  │      │
  │  │  └─────────┲─────────┘    └─────────────┬─────────────┘  │      │
  │  │            ┃ Device audio               │                │      │
  │  │            ┃ only                       │ mixed audio    │      │
  │  └────────────╋────────────────────────────┼────────────────┘      │
  │               ┃                            │                        │
  └───────────────╋────────────────────────────┼────────────────────────┘
                  ┃                            │
                  ┃                            ▼
  ┌───────────────╋─────────────────────────────────────────────────────┐
  │               ┃              AdmProxy                                │
  │               ┃    ┌────────────────────────────────────────────┐   │
  │               ┃    │                    State                    │   │
  │               ┃    │  • platform_adm_: NULL or Platform ADM      │   │
  │               ┃    │  • platform_adm_ref_count_: 0, 1, 2, ...    │   │
  │               ┃    │  • recording_enabled_: false (default)      │   │
  │               ┃    │  • playout_enabled_: false (default)        │   │
  │               ┃    └────────────────────────────────────────────┘   │
  │               ┃                                                      │
  │               ┃    ┌─────────────────────┐  ┌─────────────────────┐ │
  │               ┗━━━▶│ RecordedDataIsAvail │  │  NeedMorePlayData   │ │
  │                    │ (when enabled)      │  │  (synthetic/platform│ │
  │                    └─────────┬───────────┘  └──────────┬──────────┘ │
  └──────────────────────────────┼─────────────────────────┼────────────┘
                                 │                         │
                                 ▼                         ▼
                  ┌────────────────────────────────────────────┐
                  │        Platform ADM (Lazy Init)            │
                  │  Created when: AcquirePlatformAdm()        │
                  │  Destroyed when: ref_count → 0             │
                  └──────────────────┬─────────────────────────┘
                                     │
                          ┌──────────┴──────────┐
                          ▼                     ▼
                   ┌────────────┐        ┌────────────┐
                   │ Microphone │        │  Speakers  │
                   │  (Input)   │        │  (Output)  │
                   └────────────┘        └────────────┘
                               HARDWARE
```

### Component Relationships

```
                      Component Relationship Diagram

  ┌─────────────────────────────────────────────────────────────────┐
  │                         Rust Layer                               │
  │                                                                  │
  │  ┌─────────────┐      ┌─────────────────┐    ┌────────────────┐ │
  │  │PlatformAudio│─────▶│ LkRuntime       │───▶│PeerConnFactory │ │
  │  │             │      │                 │    │                │ │
  │  │ • rtc_source│      │• acquire_adm()  │    │• adm_proxy()   │ │
  │  │ • devices() │      │• release_adm()  │    │                │ │
  │  └─────────────┘      │• set_recording_ │    └───────┬────────┘ │
  │                       │  enabled()      │            │          │
  │  ┌─────────────┐      └─────────────────┘            │ FFI      │
  │  │NativeAudio  │                                     │          │
  │  │Source       │─────────────────────────┐           │          │
  │  │             │                         │           │          │
  │  │• capture_   │                         │           │          │
  │  │  frame()    │                         │           │          │
  │  └─────────────┘                         │           │          │
  └──────────────────────────────────────────┼───────────┼──────────┘
                                             │           │
  ┌──────────────────────────────────────────┼───────────┼──────────┐
  │                    C++ Layer (webrtc-sys)│           │          │
  │                                          │           ▼          │
  │  ┌────────────────────┐                  │   ┌───────────────┐  │
  │  │ AudioTrackSource   │◀─────────────────┘   │   AdmProxy    │  │
  │  │ (InternalSource)   │                      │               │  │
  │  │                    │                      │ ┌───────────┐ │  │
  │  │ is_external_source │                      │ │ref_count  │ │  │
  │  │   () = true        │                      │ │rec_enabled│ │  │
  │  └─────────┬──────────┘                      │ │play_enable│ │  │
  │            │                                 │ └───────────┘ │  │
  │            │ AddSink                         │       │       │  │
  │            ▼                                 │       │ lazy  │  │
  │  ┌──────────────────────────────────────┐   │       │ init  │  │
  │  │       WebRTC AudioSendStream         │   │       ▼       │  │
  │  │                                      │   │┌─────────────┐│  │
  │  │ Config.external_source = is_external │   ││ Platform    ││  │
  │  │                                      │   ││ ADM         ││  │
  │  │ if external:                         │   ││             ││  │
  │  │   → NOT added to AudioState          │   ││ CoreAudio/  ││  │
  │  │   → Audio via AddSink callbacks      │   ││ WASAPI/     ││  │
  │  │                                      │   ││ PulseAudio  ││  │
  │  │ if NOT external:                     │   │└─────────────┘│  │
  │  │   → Added to AudioState              │   │               │  │
  │  │   → Audio from ADM recording         │   │               │  │
  │  └──────────────────────────────────────┘   └───────────────┘  │
  └─────────────────────────────────────────────────────────────────┘

  Key Insight: The external_audio_source.patch enables clean separation:
  • NativeAudioSource sets is_external_source() = true
  • AudioSendStream checks this and sets Config.external_source = true
  • AudioState SKIPS streams with external_source = true
  • Result: No mixing conflict between device audio and manual push
```

### Key Components

1. **AdmProxy**: Wraps WebRTC's platform ADM with a recording gate
2. **PlatformAudio**: Rust API for enabling platform audio and device management
3. **NativeAudioSource**: Existing API for manual audio frame pushing
4. **external_audio_source.patch**: WebRTC patch to prevent audio mixing conflicts

### Lazy Initialization + Reference Counting Pattern

The Platform ADM is **not** created at startup. Instead, it's created lazily when first needed:

```cpp
// adm_proxy.h
class AdmProxy : public webrtc::AudioDeviceModule {
  // Platform ADM is created lazily, NOT at startup
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_;

  // Reference count for Platform ADM lifecycle
  int platform_adm_ref_count_ = 0;

  // Gate controls whether microphone recording is active
  // Default: FALSE - NativeAudioSource works without interference
  bool recording_enabled_ = false;

  // Gate controls whether playout goes through platform speakers
  // Default: FALSE - synthetic mode (FFI callbacks to application)
  bool playout_enabled_ = false;
};
```

**Lifecycle Management:**

| Method | Effect |
|--------|--------|
| `AcquirePlatformAdm()` | Increments ref_count. Creates Platform ADM on first call. |
| `ReleasePlatformAdm()` | Decrements ref_count. Terminates Platform ADM when count reaches 0. |

**Why Lazy Initialization?**

On iOS, creating Platform ADM configures the AVAudioSession for VoIP mode. This interferes with Unity's AudioSource playback even when PlatformAudio isn't being used. By deferring Platform ADM creation until actually needed, synthetic mode works correctly.

### Recording/Playout Gates

When Platform ADM is active, additional gates control behavior:

**When `recording_enabled_ = false` (default):**
- `InitRecording()` returns success but does nothing
- `StartRecording()` returns success but does nothing
- Microphone is not accessed
- `NativeAudioSource` works normally

**When `recording_enabled_ = true` (via `PlatformAudio::new()`):**
- `InitRecording()` initializes the microphone
- `StartRecording()` starts microphone capture
- Device audio flows to tracks using `RtcAudioSource::Device`

**When `playout_enabled_ = false` (default - synthetic mode):**
- WebRTC's audio pipeline still runs
- `NeedMorePlayData()` is called to keep pipeline alive
- Remote audio is delivered via FFI callbacks (e.g., Unity AudioSource)

**When `playout_enabled_ = true` (via `PlatformAudio::new()`):**
- Remote audio plays through platform speakers
- AEC uses playout as reference signal

### Synthetic Playout Mode

When Platform ADM is not active (or `playout_enabled_ = false`), the SDK uses synthetic playout to keep WebRTC's audio pipeline functioning:

```cpp
// AdmProxy runs a periodic task that pulls audio from WebRTC
void AdmProxy::StartSyntheticPlayoutTask() {
  // 10ms task interval (100 calls/second)
  stub_audio_queue_->PostDelayedTask(
    [this] {
      if (playing_ && audio_transport_) {
        // Pull audio from WebRTC to keep pipeline alive
        audio_transport_->NeedMorePlayData(
            kSamplesPer10Ms, kBytesPerSample, kChannels,
            kSampleRate, stub_data_.data(), samples_out,
            &elapsed_time_ms, &ntp_time_ms);
      }
      // Reschedule
      StartSyntheticPlayoutTask();
    },
    TimeDelta::Millis(10));
}
```

**Why Synthetic Playout is Needed:**

1. **Keep WebRTC Pipeline Alive**: Without playout, WebRTC's audio mixer and decoder may stop working
2. **Enable FFI Callbacks**: Remote audio is delivered to `NativeAudioStream` sinks for Unity/application handling
3. **No Platform Interference**: AVAudioSession (iOS) is not configured for VoIP mode

**Audio Modes Summary:**

| Mode | Recording | Playout | Platform ADM | Use Case |
|------|-----------|---------|--------------|----------|
| Synthetic | NativeAudioSource | FFI callbacks | Not created | Unity audio, agents |
| Platform | ADM microphone | ADM speakers | Active | VoIP with AEC |
| Hybrid | Both supported | Both supported | Active | Mixed scenarios |

---

## User Flow Diagrams

### Flow 1: Synthetic Mode (Default - NativeAudioSource + FFI Callbacks)

This is the default mode used by Unity, agents, and applications that manage their own audio I/O.

```
                  SYNTHETIC MODE - Audio Flow Diagram

  ══════════════════════════ OUTBOUND AUDIO ══════════════════════════

  ┌─────────────────┐
  │ Unity/App Code  │
  │ (TTS, file, etc)│
  └────────┬────────┘
           │ PCM audio frames
           ▼
  ┌─────────────────┐       ┌──────────────────┐       ┌─────────────────┐
  │NativeAudioSource│──────▶│ AudioTrackSource │──────▶│ AudioSendStream │
  │                 │       │  InternalSource  │       │  external=true  │
  │ capture_frame() │       │ is_external=true │       │                 │
  └─────────────────┘       └──────────────────┘       └────────┬────────┘
                                                                │
     Note: AudioState does NOT receive this audio because       │
     external=true. Audio flows directly via AddSink callbacks. │
                                                                │
                                                                ▼
                                                     ┌───────────────────┐
                                                     │    RTP Encoder    │
                                                     │    → Network      │
                                                     └───────────────────┘


  ══════════════════════════ INBOUND AUDIO ═══════════════════════════

  ┌───────────────────┐       ┌──────────────────┐
  │    Network        │──────▶│ AudioReceiveStream│
  │    RTP Decoder    │       │ (decoded audio)  │
  └───────────────────┘       └────────┬─────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    AudioMixer    │
                              │  (in AudioState) │
                              └────────┬─────────┘
                                       │
                                       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │                           AdmProxy                               │
  │                                                                  │
  │   platform_adm_ = NULL  ◀─── Platform ADM NOT created            │
  │   playout_enabled_ = false                                       │
  │                                                                  │
  │   ┌───────────────────────────────────────────────────────────┐ │
  │   │           Synthetic Playout Task (10ms interval)          │ │
  │   │                                                           │ │
  │   │  NeedMorePlayData() ─────▶ Pulls audio from mixer         │ │
  │   │                           (audio NOT sent to speakers)    │ │
  │   │                           (keeps WebRTC pipeline alive)   │ │
  │   └───────────────────────────────────────────────────────────┘ │
  └─────────────────────────────────────────────────────────────────┘

  Meanwhile, application receives audio via FFI:

  ┌──────────────────┐       ┌──────────────────┐       ┌─────────────────┐
  │ RemoteAudioTrack │──────▶│NativeAudioStream │──────▶│ Unity/App Code  │
  │                  │       │  (FFI callback)  │       │ (AudioSource)   │
  │ rtc_track.       │       │                  │       │                 │
  │  add_sink()      │       │  OnData() ───────┼──────▶│ Play via Unity  │
  └──────────────────┘       └──────────────────┘       └─────────────────┘


  KEY POINTS:
  • Platform ADM is NEVER created in synthetic mode
  • No interference with Unity AudioSource or application audio routing
  • iOS: AVAudioSession NOT configured for VoIP → Unity audio works
  • Synthetic playout task keeps WebRTC pipeline alive
```

### Flow 2: Platform Audio Mode (PlatformAudio with ADM)

This mode is used for VoIP applications that need AEC and direct microphone/speaker access.

```
                   PLATFORM MODE - Audio Flow Diagram

  ═══════════════════════════ INITIALIZATION ═════════════════════════

  ┌─────────────────┐
  │ PlatformAudio:: │
  │   new()         │
  └────────┬────────┘
           │
           ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ 1. runtime.acquire_platform_adm()                                │
  │    └─▶ AdmProxy::AcquirePlatformAdm()                            │
  │        └─▶ ref_count++ (1)                                       │
  │        └─▶ CreatePlatformAdm() [first time only]                 │
  │            └─▶ webrtc::AudioDeviceModule::Create()               │
  │            └─▶ platform_adm_->Init()                             │
  │                                                                  │
  │ 2. runtime.set_adm_recording_enabled(true)                       │
  │    └─▶ AdmProxy::set_recording_enabled(true)                     │
  │                                                                  │
  │ 3. runtime.set_adm_playout_enabled(true)                         │
  │    └─▶ AdmProxy::set_playout_enabled(true)                       │
  └─────────────────────────────────────────────────────────────────┘


  ══════════════════════════ OUTBOUND AUDIO ══════════════════════════

  ┌─────────────────────────────────────────────────────────────────┐
  │                           AdmProxy                               │
  │                                                                  │
  │   platform_adm_ = [Active Platform ADM]                          │
  │   recording_enabled_ = true                                      │
  │   playout_enabled_ = true                                        │
  │                                                                  │
  │            ┌──────────────────┐                                  │
  │            │   Platform ADM   │                                  │
  │            │  (CoreAudio/     │                                  │
  │            │   WASAPI/etc)    │                                  │
  │            └────────┬─────────┘                                  │
  │                     │ RecordedDataIsAvailable                    │
  │                     ▼                                            │
  └─────────────────────────────────────────────────────────────────┘
                        │
                        │ Microphone PCM
                        ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │                        AudioState                                │
  │                                                                  │
  │   SendingStreams[] ─────▶ Only streams with external=false       │
  │                          receive this ADM audio                  │
  └──────────────────────────────────┬──────────────────────────────┘
                                     │
                                     ▼
                           ┌──────────────────┐
                           │ AudioSendStream  │
                           │ external=false   │
                           │ (Device source)  │
                           └────────┬─────────┘
                                    │
                                    ▼
                           ┌───────────────────┐
                           │    RTP Encoder    │
                           │    → Network      │
                           └───────────────────┘


  ══════════════════════════ INBOUND AUDIO ═══════════════════════════

  ┌───────────────────┐       ┌──────────────────┐
  │    Network        │──────▶│ AudioReceiveStream│
  │    RTP Decoder    │       │ (decoded audio)  │
  └───────────────────┘       └────────┬─────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    AudioMixer    │
                              │  (in AudioState) │
                              └────────┬─────────┘
                                       │
                                       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │                           AdmProxy                               │
  │                                                                  │
  │   playout_enabled_ = true                                        │
  │                                                                  │
  │   NeedMorePlayData() ─────▶ Delegates to platform_adm_           │
  │                             ─────▶ Audio plays to speakers       │
  │                             ─────▶ AEC uses this as reference    │
  └─────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    Speakers      │
                              │    (Hardware)    │
                              └──────────────────┘


  KEY POINTS:
  • Platform ADM is CREATED when PlatformAudio::new() is called
  • Microphone audio captured by ADM → routed via AudioState → sent over network
  • Remote audio played directly to speakers via ADM
  • AEC works because playout goes through ADM (reference signal available)
  • iOS: AVAudioSession configured for VoIP mode
```

---

## Platform ADM Lifecycle

### Lifecycle State Diagram

```
                    Platform ADM Lifecycle States


               ┌─────────────────────────────────────┐
               │                                     │
               │        SYNTHETIC MODE               │
               │        (Default State)              │
               │                                     │
               │  • platform_adm_ = NULL             │
               │  • platform_adm_ref_count_ = 0      │
               │  • recording_enabled_ = false       │
               │  • playout_enabled_ = false         │
               │                                     │
               │  AdmProxy handles all ADM calls:    │
               │  - Recording ops → no-op (success)  │
               │  - Playout ops → synthetic task     │
               │                                     │
               └──────────────────┬──────────────────┘
                                  │
                PlatformAudio::new()
                └─▶ acquire_platform_adm()
                └─▶ ref_count = 1
                └─▶ CreatePlatformAdm()
                                  │
                                  ▼
               ┌─────────────────────────────────────┐
               │                                     │
               │        PLATFORM MODE                │
               │        (ADM Active)                 │
               │                                     │
               │  • platform_adm_ = [Platform ADM]   │
               │  • platform_adm_ref_count_ >= 1     │
               │  • recording_enabled_ = true        │
               │  • playout_enabled_ = true          │
               │                                     │
               │  AdmProxy delegates to platform_adm_│
               │  - Recording → real microphone      │
               │  - Playout → real speakers          │
               │                                     │
               └──────────────────┬──────────────────┘
                                  │
                drop(PlatformAudio)
                └─▶ release_platform_adm()
                └─▶ ref_count = 0
                └─▶ TerminatePlatformAdm()
                                  │
                                  ▼
               ┌─────────────────────────────────────┐
               │                                     │
               │        SYNTHETIC MODE               │
               │        (Back to Default)            │
               │                                     │
               │  • platform_adm_ = NULL             │
               │  • platform_adm_ref_count_ = 0      │
               │  • recording_enabled_ = false       │
               │  • playout_enabled_ = false         │
               │                                     │
               └─────────────────────────────────────┘
```

### Reference Counting Scenarios

```
                       Reference Counting Examples

  SCENARIO 1: Single User
  ════════════════════════

    Time ──────────────────────────────────────────────────────────▶

    ┌──────────────────────┐                    ┌──────────────────────┐
    │ PlatformAudio::new() │                    │ drop(audio)          │
    │ ref_count: 0 → 1     │                    │ ref_count: 1 → 0     │
    │ CREATE Platform ADM  │                    │ TERMINATE Platform   │
    └──────────┬───────────┘                    └──────────┬───────────┘
               │                                           │
               ▼                                           ▼
    ═══════════╪═══════════════════════════════════════════╪═══════════
    Synthetic  │         Platform Mode Active              │  Synthetic
               │                                           │

  ─────────────────────────────────────────────────────────────────────

  SCENARIO 2: Multiple Users (Shared ADM)
  ═══════════════════════════════════════

    Time ──────────────────────────────────────────────────────────▶

    ┌───────────┐     ┌───────────┐     ┌───────────┐     ┌───────────┐
    │ audio1 =  │     │ audio2 =  │     │drop(audio1│     │drop(audio2│
    │ new()     │     │ new()     │     │           │     │           │
    │ ref: 0→1  │     │ ref: 1→2  │     │ ref: 2→1  │     │ ref: 1→0  │
    │ CREATE    │     │ (reuse)   │     │ (still    │     │ TERMINATE │
    │ ADM       │     │           │     │  active)  │     │ ADM       │
    └─────┬─────┘     └─────┬─────┘     └─────┬─────┘     └─────┬─────┘
          │                 │                 │                 │
          ▼                 ▼                 ▼                 ▼
    ══════╪═════════════════╪═════════════════╪═════════════════╪══════
    Synth │    Platform Mode Active           │                 │ Synth
          │                                   │ audio2 still    │
          │                                   │ works!          │

  ─────────────────────────────────────────────────────────────────────

  SCENARIO 3: FFI Clients (Unity/Python)
  ═════════════════════════════════════

    ┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
    │ Unity Client A  │       │ Unity Client B  │       │ Python Agent    │
    │                 │       │                 │       │                 │
    │ NewPlatformAudio│       │ NewPlatformAudio│       │                 │
    │ Request         │       │ Request         │       │                 │
    │ handle_1        │       │ handle_2        │       │                 │
    └────────┬────────┘       └────────┬────────┘       │ Uses Native     │
             │                         │                │ AudioSource     │
             │                         │                │ (no PlatformAdm)│
             ▼                         ▼                └─────────────────┘
    ┌─────────────────────────────────────────────────────────────────┐
    │                        AdmProxy                                  │
    │                                                                  │
    │   platform_adm_ref_count_ = 2  (from Unity clients)              │
    │                                                                  │
    │   Both Unity clients share the same Platform ADM.                │
    │   Python agent uses synthetic mode (NativeAudioSource).          │
    │                                                                  │
    │   When both Unity clients call DisposeRequest:                   │
    │     handle_1 dispose → ref_count = 1                             │
    │     handle_2 dispose → ref_count = 0 → TERMINATE                 │
    └─────────────────────────────────────────────────────────────────┘
```

### Why Lazy Initialization Matters

```
                iOS Audio Session Problem & Solution

  ═══════════════════ WITHOUT LAZY INIT (Problem) ═══════════════════

  App Startup
  ────────────
       │
       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ PeerConnectionFactory created                                    │
  │ └─▶ AdmProxy created                                             │
  │     └─▶ Platform ADM created immediately                         │
  │         └─▶ iOS: AVAudioSession configured for VoIP mode         │
  │             └─▶ Audio session category = PlayAndRecord           │
  │                 └─▶ Other audio (Unity AudioSource) INTERRUPTED  │
  └─────────────────────────────────────────────────────────────────┘
       │
       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ Unity app tries to play audio via AudioSource                    │
  │ ❌ FAILS - AVAudioSession is in VoIP mode!                       │
  └─────────────────────────────────────────────────────────────────┘


  ════════════════════ WITH LAZY INIT (Solution) ════════════════════

  App Startup
  ────────────
       │
       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ PeerConnectionFactory created                                    │
  │ └─▶ AdmProxy created                                             │
  │     └─▶ platform_adm_ = NULL (NOT created!)                      │
  │         └─▶ iOS: AVAudioSession NOT configured                   │
  │             └─▶ Other audio works normally!                      │
  └─────────────────────────────────────────────────────────────────┘
       │
       ▼
  ┌─────────────────────────────────────────────────────────────────┐
  │ Unity app plays audio via AudioSource                            │
  │ ✅ WORKS - using synthetic mode, no ADM interference             │
  └─────────────────────────────────────────────────────────────────┘
       │
       ▼ (Later, if needed)
  ┌─────────────────────────────────────────────────────────────────┐
  │ PlatformAudio::new() called for VoIP                             │
  │ └─▶ acquire_platform_adm()                                       │
  │     └─▶ CreatePlatformAdm() - NOW creates Platform ADM           │
  │         └─▶ iOS: AVAudioSession configured for VoIP              │
  │                                                                  │
  │ drop(PlatformAudio)                                              │
  │ └─▶ release_platform_adm()                                       │
  │     └─▶ TerminatePlatformAdm() - destroys Platform ADM           │
  │         └─▶ iOS: AVAudioSession released                         │
  │             └─▶ Unity audio can work again!                      │
  └─────────────────────────────────────────────────────────────────┘
```

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

## Remote Audio Playback

Understanding how remote audio reaches speakers is important for choosing the right audio mode.

### Without PlatformAudio (Manual Playback)

When using only `NativeAudioSource` (the default mode), remote audio does **not** automatically play to speakers. You must explicitly create an `AudioStream` to receive audio frames from remote tracks:

```rust
use livekit::prelude::*;
use libwebrtc::audio_stream::native::NativeAudioStream;
use futures_util::StreamExt;

// When a remote track is received
let RoomEvent::TrackSubscribed { track, .. } = event else { continue };
let RemoteTrack::Audio(remote_audio) = track.into() else { continue };

// Create an AudioStream to pull audio from the remote track
let mut stream = NativeAudioStream::new(
    remote_audio.rtc_track(),
    48000,  // desired sample rate
    2,      // desired channels
);

// Poll the stream to receive audio frames
while let Some(frame) = stream.next().await {
    // frame.data: Vec<i16> - PCM audio samples
    // frame.sample_rate: u32
    // frame.num_channels: u32
    // frame.samples_per_channel: u32

    // Application must route this audio to speakers manually
    // (e.g., via cpal, rodio, or platform audio APIs)
}
```

**How it works internally:**

1. `NativeAudioStream::new()` creates a `NativeAudioSink` and registers it with the remote track via `audio.add_sink(&sink)`
2. WebRTC calls the sink's `on_data()` callback when decoded audio frames arrive
3. Frames are queued (bounded queue with configurable size, default 10 frames / ~100ms)
4. Application polls the stream to receive frames
5. Application is responsible for routing audio to the actual speaker device

**Use case:** Server-side agents, headless applications, or apps that need custom audio routing.

### With PlatformAudio (Automatic Playback)

When `PlatformAudio` is active, remote audio automatically plays through the system speakers via WebRTC's audio mixer and the ADM's playout path:

```rust
use livekit::prelude::*;

// Create PlatformAudio (enables both recording AND playout via ADM)
let audio = PlatformAudio::new()?;

// Optionally select speaker device
audio.set_playout_device(0)?;

// Connect to room - remote audio will automatically play through speakers
let (room, mut events) = Room::connect(&url, &token, RoomOptions::default()).await?;

// Remote tracks automatically play - no AudioStream needed for speaker output
while let Some(event) = events.recv().await {
    match event {
        RoomEvent::TrackSubscribed { track, .. } => {
            // Audio track automatically plays to speakers
            // No additional code needed for playback
        }
        _ => {}
    }
}
```

**How it works internally:**

1. WebRTC's `AudioReceiveStream` decodes incoming audio
2. Audio is mixed by WebRTC's internal audio mixer
3. ADM's `NeedMorePlayData()` is called by the audio device thread
4. Mixed audio is delivered to the platform speaker device

**Track mute/unmute:** Remote track mute state is handled by WebRTC internally. Muted tracks don't contribute to the mix.

### Comparison

| Aspect | Without PlatformAudio | With PlatformAudio |
|--------|----------------------|-------------------|
| Remote audio to speakers | Manual via `NativeAudioStream` | Automatic via ADM |
| Application code needed | Create stream + route to speaker | None |
| Latency | Depends on app implementation | Optimized by WebRTC |
| Audio mixing | Application handles | WebRTC handles |
| Device selection | Application handles | `set_playout_device()` |
| AEC reference | Not available | Available |

### Hybrid Approach

You can combine both approaches - use `PlatformAudio` for automatic speaker playback while also creating `NativeAudioStream` for audio processing/analysis:

```rust
let audio = PlatformAudio::new()?;  // Enables automatic playback

// Remote audio plays automatically to speakers
// Additionally, create a stream for audio analysis
let stream = NativeAudioStream::new(remote_track.rtc_track(), 48000, 1);
tokio::spawn(async move {
    while let Some(frame) = stream.next().await {
        // Analyze audio (e.g., VAD, transcription)
        // Audio still plays to speakers via ADM
    }
});
```

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

  // Platform ADM Lifecycle Management
  bool AcquirePlatformAdm();   // Increment ref, create ADM on first call
  void ReleasePlatformAdm();   // Decrement ref, terminate ADM when 0
  int platform_adm_ref_count() const;
  bool is_platform_adm_active() const;

  // Recording/Playout Control
  void set_recording_enabled(bool enabled);
  bool recording_enabled() const;
  void set_playout_enabled(bool enabled);
  bool playout_enabled() const;

  // All AudioDeviceModule methods with gated behavior

 private:
  bool CreatePlatformAdm();      // Called by AcquirePlatformAdm
  void TerminatePlatformAdm();   // Called by ReleasePlatformAdm

  void StartSyntheticPlayoutTask();  // Keep WebRTC alive in synthetic mode
  void StopSyntheticPlayoutTask();

  const webrtc::Environment env_;
  webrtc::Thread* worker_thread_;

  mutable webrtc::Mutex mutex_;

  // Platform ADM (created lazily, NOT at startup)
  webrtc::scoped_refptr<webrtc::AudioDeviceModule> platform_adm_;
  int platform_adm_ref_count_ = 0;

  // Control flags
  bool recording_enabled_ = false;  // Default: NativeAudioSource mode
  bool playout_enabled_ = false;    // Default: synthetic mode (FFI callbacks)

  // Synthetic playout task
  std::unique_ptr<webrtc::TaskQueueBase, webrtc::TaskQueueDeleter> stub_audio_queue_;
  webrtc::RepeatingTaskHandle stub_audio_task_;
  std::vector<int16_t> stub_data_;
};
```

### Lazy Initialization Implementation

```cpp
// webrtc-sys/src/adm_proxy.cpp

AdmProxy::AdmProxy(const webrtc::Environment& env, webrtc::Thread* worker_thread)
    : env_(env), worker_thread_(worker_thread), stub_data_(kSamplesPer10Ms * kChannels) {
  // Platform ADM is NOT created here - lazy initialization
  RTC_LOG(LS_INFO) << "AdmProxy: Lazy initialization mode (no Platform ADM yet)";
}

bool AdmProxy::AcquirePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);
  platform_adm_ref_count_++;

  if (platform_adm_ref_count_ == 1) {
    // First acquisition - create Platform ADM
    if (!CreatePlatformAdm()) {
      platform_adm_ref_count_--;
      return false;
    }
    RTC_LOG(LS_INFO) << "AdmProxy: Platform ADM created and initialized";
  }
  return platform_adm_ != nullptr;
}

void AdmProxy::ReleasePlatformAdm() {
  webrtc::MutexLock lock(&mutex_);
  if (platform_adm_ref_count_ <= 0) return;

  platform_adm_ref_count_--;
  if (platform_adm_ref_count_ == 0) {
    // Last release - terminate Platform ADM
    TerminatePlatformAdm();
    RTC_LOG(LS_INFO) << "AdmProxy: Platform ADM terminated, returning to synthetic mode";
  }
}
```

### Recording/Playout Gate Implementation

```cpp
int32_t AdmProxy::InitRecording() {
  webrtc::MutexLock lock(&mutex_);
  if (!recording_enabled_ || !platform_adm_) {
    recording_initialized_ = true;  // Track state even in synthetic mode
    return 0;  // Success but no-op
  }
  return platform_adm_->InitRecording();
}

int32_t AdmProxy::StartPlayout() {
  webrtc::MutexLock lock(&mutex_);
  if (!playout_enabled_ || !platform_adm_) {
    // Synthetic mode - start stub task to keep WebRTC pipeline alive
    playing_ = true;
    StartSyntheticPlayoutTask();
    return 0;
  }
  return platform_adm_->StartPlayout();
}
```

### PlatformAudio Reference Counting

```rust
// livekit/src/platform_audio/mod.rs

lazy_static! {
    static ref PLATFORM_ADM_HANDLE: Mutex<Weak<PlatformAdmHandle>> = Mutex::new(Weak::new());
}

struct PlatformAdmHandle {
    runtime: Arc<LkRuntime>,
}

impl Drop for PlatformAdmHandle {
    fn drop(&mut self) {
        // Release Platform ADM reference when last PlatformAudio is dropped
        self.runtime.release_platform_adm();
        log::info!("PlatformAdmHandle: released Platform ADM");
    }
}

impl PlatformAudio {
    pub fn new() -> AudioResult<Self> {
        let mut handle_ref = PLATFORM_ADM_HANDLE.lock();

        // Reuse existing handle if available
        if let Some(handle) = handle_ref.upgrade() {
            // Still acquire Platform ADM for this instance
            handle.runtime.acquire_platform_adm();
            return Ok(Self { handle });
        }

        // Create new handle and acquire Platform ADM
        let runtime = LkRuntime::instance();

        // Acquire Platform ADM - creates it on first call
        if !runtime.acquire_platform_adm() {
            return Err(AudioError::PlatformInitFailed);
        }

        // Enable recording and playout for platform audio mode
        runtime.set_adm_recording_enabled(true);
        runtime.set_adm_playout_enabled(true);

        let handle = Arc::new(PlatformAdmHandle { runtime });
        *handle_ref = Arc::downgrade(&handle);

        Ok(Self { handle })
    }
}
```

### Lifecycle Scenarios

**Scenario 1: Single PlatformAudio**
```
PlatformAudio::new()  → acquire_platform_adm() → ref_count=1, ADM created
drop(audio)           → release_platform_adm() → ref_count=0, ADM terminated
```

**Scenario 2: Multiple PlatformAudio Instances**
```
audio1 = PlatformAudio::new()  → ref_count=1, ADM created
audio2 = PlatformAudio::new()  → ref_count=2, reuse ADM
drop(audio1)                   → ref_count=1, ADM still active
drop(audio2)                   → ref_count=0, ADM terminated
```

**Scenario 3: Device Enumeration Then Release**
```
audio = PlatformAudio::new()   → ref_count=1, ADM created
devices = audio.recording_devices()
drop(audio)                    → ref_count=0, ADM terminated
// Synthetic mode now works correctly - ADM not interfering
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
