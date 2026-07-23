# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.32](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.31...rust-sdks/livekit@0.7.32) - 2026-02-16

### Fixed

- fix full_reconnect downgrade & don't ignore Leave messages ([#893](https://github.com/livekit/rust-sdks/pull/893))

### Other

- turn single peerconnection off by default ([#897](https://github.com/livekit/rust-sdks/pull/897))
- ensure signal connections times out properly and retries ([#895](https://github.com/livekit/rust-sdks/pull/895))
- added Single Peer Connection support to Rust ([#888](https://github.com/livekit/rust-sdks/pull/888))
- set the simulcast codec & layers ([#891](https://github.com/livekit/rust-sdks/pull/891))

## [0.7.31](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.30...rust-sdks/livekit@0.7.31) - 2026-02-10

### Other

- don't use clamp as the ultimate_kbps can be lower than 300 ([#886](https://github.com/livekit/rust-sdks/pull/886))
- pre-connect the publisher PC when an RPC handler is registered ([#880](https://github.com/livekit/rust-sdks/pull/880))

## [0.7.30](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.29...rust-sdks/livekit@0.7.30) - 2026-02-09

### Fixed

- fix the RPC race condition ([#865](https://github.com/livekit/rust-sdks/pull/865))

### Other

- update proto & fix CI ([#871](https://github.com/livekit/rust-sdks/pull/871))
- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Upgrade protocol to v1.44.0 ([#857](https://github.com/livekit/rust-sdks/pull/857))
- Expose participant's permission to ffi layer ([#824](https://github.com/livekit/rust-sdks/pull/824))

## [0.7.29](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.28...rust-sdks/livekit@0.7.29) - 2026-01-15

### Fixed

- ensure Room.creation_time is ms ([#822](https://github.com/livekit/rust-sdks/pull/822))

### Other

- try setting x-google-start-bitrate for vp9 ([#820](https://github.com/livekit/rust-sdks/pull/820))

## [0.7.28](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.27...rust-sdks/livekit@0.7.28) - 2025-12-19

### Added

- *(ParticipantInfo)* export kind details ([#813](https://github.com/livekit/rust-sdks/pull/813))

## [0.7.27](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.26...rust-sdks/livekit@0.7.27) - 2025-12-17

### Other

- Handle server initiated mute request ([#812](https://github.com/livekit/rust-sdks/pull/812))

## [0.7.26](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.25...rust-sdks/livekit@0.7.26) - 2025-12-04

### Added

- *(connector)* initial service impl ([#790](https://github.com/livekit/rust-sdks/pull/790))

### Fixed

- fix mute/unmute events for LocalTrack. ([#799](https://github.com/livekit/rust-sdks/pull/799))

### Other

- Add RoomEvent::TokenRefreshed ([#803](https://github.com/livekit/rust-sdks/pull/803))

## [0.7.25](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.24...rust-sdks/livekit@0.7.25) - 2025-11-20

### Other

- perform full reconnect if resume fails ([#792](https://github.com/livekit/rust-sdks/pull/792))
- E2E RPC tests ([#769](https://github.com/livekit/rust-sdks/pull/769))
- Remove unused dependencies ([#761](https://github.com/livekit/rust-sdks/pull/761))

## [0.7.24](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.23...rust-sdks/livekit@0.7.24) - 2025-10-27

### Other

- Expose set video quality ([#759](https://github.com/livekit/rust-sdks/pull/759))

## [0.7.23](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.22...rust-sdks/livekit@0.7.23) - 2025-10-23

### Other

- add h265 codec support ([#762](https://github.com/livekit/rust-sdks/pull/762))

## [0.7.22](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.21...rust-sdks/livekit@0.7.22) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- Remove participant check for data packets ([#757](https://github.com/livekit/rust-sdks/pull/757))
- clamp connection timeout and fixed the comment ([#748](https://github.com/livekit/rust-sdks/pull/748))
- put examples in root Cargo workspace ([#731](https://github.com/livekit/rust-sdks/pull/731))

## [0.7.21](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.20...rust-sdks/livekit@0.7.21) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Fixed

- fix some potential audio issues, clean up the code a bit, and suppress some warnings  ([#737](https://github.com/livekit/rust-sdks/pull/737))
- do not log 'signal client closed: "stream closed"' on disconnect ([#727](https://github.com/livekit/rust-sdks/pull/727))

### Other

- Test participant disconnect ([#732](https://github.com/livekit/rust-sdks/pull/732))
- Increase RPC max RT time to 7s ([#729](https://github.com/livekit/rust-sdks/pull/729))
- E2E audio test ([#724](https://github.com/livekit/rust-sdks/pull/724))

## [0.7.20](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.19...rust-sdks/livekit@0.7.20) - 2025-10-03

### Other

- updated the following local packages: libwebrtc

## [0.7.19](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.18...rust-sdks/livekit@0.7.19) - 2025-09-29

### Fixed

- apply original participant fields in data messages ([#709](https://github.com/livekit/rust-sdks/pull/709))

### Other

- Implement Display and Error for RpcError ([#719](https://github.com/livekit/rust-sdks/pull/719))
- Fix intermittently failing E2E reliability test ([#718](https://github.com/livekit/rust-sdks/pull/718))
- Do not modify raw packets ([#714](https://github.com/livekit/rust-sdks/pull/714))
- Add send_bytes method ([#691](https://github.com/livekit/rust-sdks/pull/691))
- Disable opus red for e2ee enabled clients ([#706](https://github.com/livekit/rust-sdks/pull/706))
- Upgrade protocol to v1.41.0 ([#703](https://github.com/livekit/rust-sdks/pull/703))

## [0.7.18](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.17...rust-sdks/livekit@0.7.18) - 2025-09-09

### Other

- Add option to enable audio preconnect buffer in SDK & FFI ([#700](https://github.com/livekit/rust-sdks/pull/700))
- Data channel reliability ([#688](https://github.com/livekit/rust-sdks/pull/688))

## [0.7.17](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.16...rust-sdks/livekit@0.7.17) - 2025-09-03

### Other

- updated the following local packages: livekit-api, libwebrtc

## [0.7.16](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.15...rust-sdks/livekit@0.7.16) - 2025-07-31

### Other

- updated the following local packages: livekit-api
# Changelog

## [0.7.15](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.14...rust-sdks/livekit@0.7.15) - 2025-07-16

### Other

- remove published tracks when the room is closed ([#677](https://github.com/livekit/rust-sdks/pull/677))

## [0.7.14](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.13...rust-sdks/livekit@0.7.14) - 2025-06-23

### Fixed

- `audio_frame_ms` didn't work expectedly ([#671](https://github.com/livekit/rust-sdks/pull/671))

## [0.7.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.12...rust-sdks/livekit@0.7.13) - 2025-06-17

### Other

- Expose room updates, support MoveParticipant (protocol 15) ([#662](https://github.com/livekit/rust-sdks/pull/662))

## [0.7.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit@0.7.11...rust-sdks/livekit@0.7.12) - 2025-06-11

### Fixed

- fix duration overflow ([#654](https://github.com/livekit/rust-sdks/pull/654))

### Other

- Remove debouncer when fast_publish is enabled ([#649](https://github.com/livekit/rust-sdks/pull/649))

## [0.7.9] - 2025-04-08

### Added

- High-level data streams API

### Deprecated

- Low-level data stream packet events

## [0.7.8] - 2025-03-21

### Fixed

- Revert the RPC change, need more robust way

## [0.7.7] - 2025-03-18

### Fixed

- Move RPC handlers to room

## [0.7.6] - 2025-02-28

### Added

- Add audio filter support

## [0.7.5] - 2025-02-06

### Fixed

- Fix a dependency issue with an older version of the libwebrtc crate

## [0.7.4] - 2025-02-03

### Added

- Support for track subscription permissions

## [0.7.3] - 2025-01-17

### Added

- Add an API to set buffer_amount_low_threshold for DataChannel
- Update RoomInfo to contain buffer_amount_low_threshold for DataChannel

### Fixed

- Wait for the buffered amount to become low before sending data during publish_data for Reliable Data Channel

## [0.7.2] - 2025-01-04

### Added

- bump

### Fixed

- Fixed deadlock with nested RPC calls

## [0.7.1] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.8.0 (2026-07-23)

### Breaking Changes

#### Route LiveKit signalling through a pluggable transport (new `livekit-net` crate).

The signalling WebSocket and the two pre-connect HTTP GETs (validate, region discovery) now go through pluggable transport traits (`WsClient` for the WebSocket, `HttpClient` for request/response) resolved from a process-global registry with independent slots — a consumer can bring only HTTP, or only WebSocket. The new `livekit-net` crate owns the WebSocket/HTTP/TLS stack behind those traits and ships native (tokio / async-std) backends. Native builds are unchanged in behavior.

**Breaking (`livekit-api`, and `livekit` via `EngineError::Signal`):**

- `SignalError::WsError` is removed — `tungstenite` is no longer part of the public API. A failed WebSocket handshake now surfaces its HTTP status as `SignalError::Client`/`Server`; transport connection and close failures surface as the new `SignalError::Connection(String)` / `SignalError::Closed` variants (previously all collapsed into `Timeout`).
- `SignalError` is now `#[non_exhaustive]`, and gains a `SignalError::TransportNotConfigured` variant — returned when no transport is registered (host/foreign builds must call `livekit_net::set_ws_client` / `set_http_client` before connecting). This is a permanent configuration error; callers must not retry.
- The signalling WebSocket/HTTP/TLS crates are no longer transitive dependencies of `livekit-api`; TLS features delegate to `livekit-net`. Existing `signal-client-tokio` / `-async` / `-dispatcher` and TLS feature names are unchanged.

### Fixes

- Address typo in parsing rpc server version - #1268 (@1egoman)
- Emit black keepalive frames from NativeVideoSource instead of uninitialized memory. webrtc::I420Buffer::Create leaves the pixel planes uninitialized, so the pre-capture keepalive frames could leak recycled heap contents (often fragments of earlier frames from the same process) to subscribers as the first keyframes - #1271 (@eh-steve)
- Add NVIDIA NVENC AV1 encoding when the GPU reports AV1 encode support.

## 0.7.53 (2026-07-17)

### Features

- Add a pre-encoded video publish path: a passthrough video encoder and encoded video frame buffer in webrtc-sys, and `EncodedVideoFrame`/`EncodedVideoCodec`/`EncodedFrameType` publish APIs with a `VideoEncoderBackend::PreEncoded` backend in libwebrtc. WebRTC rate-control targets and keyframe requests are forwarded to encoded sources, and pre-encoded AV1 and H265 access units are validated on ingest.

### Fixes

- Emit room EOS when the underlying LiveKit room event channel closes after a server-initiated disconnect, and ignore duplicate disconnect events during teardown.
- Don't log an expected publisher data channel close as unexpected - #1224 (@longcw)

#### Simplify x-google-start-bitrate logic and update degradation preference defaults

- Start bitrate: use min(90% of target, 1 Mbps) instead of adaptive network hints
- Remove slow connection detection and network quality hints on reconnect
- Default degradation preference by track source:
  - Camera: MaintainFramerate (smoother video)
  - Screenshare: MaintainResolution (clarity for text/UI)
  - Other: Balanced

## 0.7.52 (2026-07-14)

### Fixes

- Make some fields public for data track types
- Refactor data tracks E2EE interface
- refactor: extract data-stream logic and shared types into new `livekit-common` and `livekit-data-stream` crates (public API unchanged; types are re-exported from `livekit`)
- Use concrete type for data track manager output events
- Add an opt-in zero-playout-delay mode for native video subscribers, expose it through the `local_video` subscriber's `--low-latency` flag, and isolate subscriber diagnostics from frame-driven video rendering.

## 0.7.51 (2026-07-09)

### Fixes

- feat: auto failover APIs with LK Cloud - #1196 (@davidzhao)
- Fix for dynacast error - #1213 (@MaxHeimbrock)
- Fix malformed RTC error handling
- Handle data track SID reassignment
- introduce LiveKitAPI construct, added smoke tests - #1220 (@davidzhao)
- Turn single peerconnection off by default - #1206 (@cnderrauber)

## 0.7.50 (2026-06-30)

### Features

- Add `user_data` support to frame metadata, allowing arbitrary application-supplied bytes to be attached to a video frame via the `PTF_USER_DATA` packet trailer feature.

#### Improve initial video quality by setting `x-google-start-bitrate` SDP hint for all video codecs (VP8, VP9, AV1, H264, H265) and defaulting to `MaintainResolution` degradation preference.

This addresses the issue where video starts blurry for several seconds before improving, by:
1. Telling WebRTC's bandwidth estimator to start at 70% of target bitrate instead of ramping up from ~300kbps
2. Preferring frame drops over resolution reduction when bandwidth is constrained

The `DegradationPreference` option is now exposed via FFI for Python, C++, Unity, and Node SDKs.

#### Add `MaintainFramerateAndResolution` to `DegradationPreference` enum to align with WebRTC M144.

- `MAINTAIN_FRAMERATE_AND_RESOLUTION` is now the recommended value (replaces deprecated `DISABLED`)
- `DISABLED` is deprecated but still supported for backwards compatibility
- Both values map to the same behavior: maintain framerate and resolution, dropping frames if needed

### Fixes

- Fix AV1 subscriber decode when packet trailers are enabled.
- Improve log messages around plugin loading - #1186 (@lukasIO)

## 0.7.49 (2026-06-24)

### Fixes

- harden reconnect behaviour - #1148 (@lukasIO)

## 0.7.48 (2026-06-23)

### Features

- Rename user facing APIs for Packet Trailer to Frame Metadata.

### Fixes

- Upgrade protocol to v1.48.0

## 0.7.47 (2026-06-19)

### Fixes

- fix: escalate to full reconnect if connection failed during a resume - #1175 (@davidzhao)

## 0.7.46 (2026-06-17)

### Features

#### Make GLib an opt-in dependency

`webrtc-sys` no longer links against `glib-2.0`/`gobject-2.0`/`gio-2.0` by default.

Breaking: Wayland screen sharing now requires the `glib-main-loop` feature on `livekit` (or `libwebrtc`).

### Fixes

- Add track publishing doc example
- Fix silent subscription failures in single-pc mode when the SFU reuses an existing empty transceiver for a new remote track. Also make `RtpTransceiver::mid()` safe to call on transceivers that haven't been negotiated yet — libwebrtc is built with `-fno-exceptions`, so `std::optional::value()` aborted the process instead of throwing.
- Add `LK_DISABLE_NVDEC` to bypass NVIDIA NVDEC decoder registration when the environment variable is set.
- return DeviceNotFound when device is not there for set_recording_devi… - #1155 (@xianshijing-lk)

#### Add dynacast support - #1003 (@chenosaurus, @stephen-derosa)

This includes a minor breaking change for `libwebrtc`: `RtpParameters` now
contains additional RTP sender state that must be preserved when round-tripping
through `set_parameters()`.

## 0.7.45 (2026-06-09)

### Fixes

- Fix NVIDIA encoder I420 uploads to copy each plane using its actual source stride, avoiding chroma corruption when source frames use padded YUV planes. Also fix the `local_video` publisher reusing mutable I420 frame storage after handing frames to WebRTC.
- Reject oversized data messages before they break the data channel.
- Add per-publication video encoder backend selection. Add a video encoder backend availability query. Remove `LIVEKIT_PREFERRED_HW_ENCODER` in favor of per-publication backend selection.

## 0.7.44 (2026-06-03)

### Features

- Add rpc max_round_trip_latency and move to builder pattern - #1127 (@1egoman)

### Fixes

- [allow(dead_code)] for dead function in room module - #1128 (@stephen-derosa)
- Send publisher offer with join request to accelerate connection - #996 (@cnderrauber)

## 0.7.43 (2026-05-29)

### Fixes

- bump protocol to v1.46.4 - #1121 (@lukasIO)
- Add native video pipeline timing instrumentation for local video measurements, exposing local publish and subscribe timing through async streams and subscriber overlay GPU upload and receive-to-GPU latency metrics through explicit timing observers.

## 0.7.42 (2026-05-21)

### Features

- Introduce pipeline options for remote data tracks, support multiple in-flight frames.

### Fixes

- Filter internal data streams out of livekit-ffi interface - #1112 (@1egoman)

#### feat: add Android application context initialization for PlatformAudio support.

Android requires `ContextUtils.initialize(applicationContext)` before WebRTC audio components can be created. This change:

- Adds `livekit_ffi_initialize_android_context()` C FFI function for Unity and other FFI consumers
- Uses `CreateAndroidAudioDeviceModule()` instead of generic `CreateAudioDeviceModule()` on Android
- Handles empty device GUIDs on Android (falls back to index 0)
- Documents Android-specific limitations: single default device, no app-level device selection

Platform notes:
- Android device enumeration returns only one "default" device with empty name/GUID
- Audio routing (speaker/earpiece/Bluetooth) is controlled by Android's AudioManager, not WebRTC

## 0.7.41 (2026-05-20)

### Fixes

- Bugfix: Always emit Disconnected on engine close - #1096 (@MaxHeimbrock)
- Support for large RPC messages using data streams - #1013 (@1egoman)

## 0.7.40 (2026-05-14)

### Fixes

- feat: add scalability mode for AV1/VP9. - #1076 (@cloudwebrtc)
- Add `LIVEKIT_PREFERRED_HW_ENCODER` to prefer `nvenc` or `vaapi` hardware video encoding when both are available.
- Relocate unrelated types out of `livekit-protocol`

#### Get WebRTC ADM into Rust - #1037 (@xianshijing-lk)

This PR introduces platform audio device management via WebRTC's Audio Device Module (ADM).

#### Features
- **ADM Proxy**: New `AdmProxy` class that switches between Dummy ADM (synthetic mode) and Platform ADM (real audio I/O)
- **PlatformAudio API**: High-level Rust API for microphone capture and speaker playout with AEC/AGC/NS
- **Device enumeration**: List and select recording/playout devices by index or GUID
- **Mode switching**: Seamlessly switch between synthetic mode (FFI callbacks) and platform mode (native speakers) while audio is active
- **FFI platform audio support**: Expose platform audio device enumeration and selection through `livekit-ffi`
- **Audio processing**: Configure echo cancellation, noise suppression, and auto gain control with platform-specific defaults (hardware on iOS, software elsewhere)

#### Audio Modes
| Mode | Recording | Playout | Use Case |
|------|-----------|---------|----------|
| Synthetic | NativeAudioSource | Dummy ADM + FFI | Unity audio, agents |
| Platform | Platform ADM mic | Platform ADM speakers | VoIP with AEC |

#### API
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

## 0.7.39 (2026-05-11)

### Fixes

- fix: Sync inner.enabled state for E2EE manager. - #1073 (@cloudwebrtc)
- Upgrade protocol to v1.45.8

## 0.7.38 (2026-05-10)

### Fixes

- Bump `rustls-webpki` to 0.103.13, addressing [GHSA-82j2-j2ch-gfr8](https://github.com/advisories/GHSA-82j2-j2ch-gfr8)
- Fix missing `libwebrtc.jar` for Android builds, harden build scripts
- fix: derive `simulcasted` from non-deprecated TrackInfo fields - #1052 (@cloudwebrtc)
- fix race in download_webrtc to reduce flaky build - #1047 (@hechen-eng)
- Improve WebRTC build scripts and add external_audio_source patch - #1053 (@xianshijing-lk)
- support SimulateScenario through FFI to improve testing - #1069 (@davidzhao)
- TEL-464: reduce redundant resampling in audio filter - #1019 (@hechen-eng)

## 0.7.37 (2026-04-23)

### Features

#### Add support for frame level packet trailer

##890 by @chenosaurus

- Add support to attach/parse frame level timestamps & frame ID to VideoTracks as a custom payload trailer.
- Breaking change in VideoFrame API, must include `frame_metadata` or use VideoFrame::new().

### Fixes

- Add device-info crate and send device_info to telemetry - #982 (@maxheimbrock)
- Fix data track packet format issue breaking E2EE
- Fix unbound send queue that can cause latency in data track messages - #1032 (@chenosaurus)
- Fix for raw stream drop called from non tokio thread like Unity .NET GC - #1016 (@MaxHeimbrock)

## 0.7.36 (2026-04-02)

### Features

- Initial support for data tracks

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.7.35 (2026-03-31)

### Features

- Expose participant active event, state, and joined at

### Fixes

- Upgrade to thiserror 2

## 0.7.34 (2026-03-22)

### Features

#### E2EE: allow setting key_ring_size and key_derivation_algorithm, update webrtc to m144

##921 by @onestacked

This PR uses [this webrtc-sdk PR](https://github.com/webrtc-sdk/webrtc/pull/224) to configure the KDF.

I've tested this with https://codeberg.org/esoteric_programmer/matrix-jukebox and it is compatible with Element Call.

Fixed: https://github.com/livekit/rust-sdks/issues/796

### Fixes

- Add disconnectReason to Room::close
- End-to-end testing for video streams
- Fix H.264 codec matching

#### add bounded buffer to audio_stream, and use 10 frames as the default

##945 by @xianshijing-lk

#### fix PC timeout when connecting with can_subscribe=false

##955 by @s-hamdananwar

When a participant connects with `canSubscribe=false` in their token, the server sends `subscriber_primary=false` in the JoinResponse and does not send a subscriber offer.  This results in `wait_pc_connection` timing out as it is expecting a subscriber PC even when the publisher PC is primary. This PR will skip waiting for subscriber PC when `subscriber_primary=false`.

#### Send client os and os_version from rust

##952 by @MaxHeimbrock

Adds [os_info](https://crates.io/crates/os_info) crate as dependency and sends the data for client connections.

## 0.7.33 (2026-03-13)

### Fixes

#### enhanced build configuration to support macOS and iOS platforms with proper system library linking

##847 by @SchmErik

#### fix video track subscription in single peer connection mode

##914 by @xianshijing-lk
