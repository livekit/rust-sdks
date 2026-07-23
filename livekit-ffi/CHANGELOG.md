# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.12.47](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.46...rust-sdks/livekit-ffi@0.12.47) - 2026-02-10

### Other

- update Cargo.toml dependencies
- don't use clamp as the ultimate_kbps can be lower than 300 ([#886](https://github.com/livekit/rust-sdks/pull/886))
- pre-connect the publisher PC when an RPC handler is registered ([#880](https://github.com/livekit/rust-sdks/pull/880))

## [0.12.46](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.45...rust-sdks/livekit-ffi@0.12.46) - 2026-02-09

### Other

- update Cargo.toml dependencies

## [0.12.45](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.44...rust-sdks/livekit-ffi@0.12.45) - 2026-02-09

### Fixed

- fix the RPC race condition ([#865](https://github.com/livekit/rust-sdks/pull/865))

### Other

- update proto & fix CI ([#871](https://github.com/livekit/rust-sdks/pull/871))
- add can_manage_agent_session permission ([#870](https://github.com/livekit/rust-sdks/pull/870))
- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Upgrade protocol to v1.44.0 ([#857](https://github.com/livekit/rust-sdks/pull/857))
- Use dedicated audio_runtime with high priority for audio capture  ([#854](https://github.com/livekit/rust-sdks/pull/854))
- Expose participant's permission to ffi layer ([#824](https://github.com/livekit/rust-sdks/pull/824))
- Add a request_async_id to the async requests ([#842](https://github.com/livekit/rust-sdks/pull/842))
- Use the correct download url in webrtc-sys build. ([#825](https://github.com/livekit/rust-sdks/pull/825))

## [0.12.44](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.43...rust-sdks/livekit-ffi@0.12.44) - 2026-01-15

### Fixed

- ensure Room.creation_time is ms ([#822](https://github.com/livekit/rust-sdks/pull/822))

### Other

- try setting x-google-start-bitrate for vp9 ([#820](https://github.com/livekit/rust-sdks/pull/820))

## [0.12.43](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.42...rust-sdks/livekit-ffi@0.12.43) - 2025-12-19

### Added

- *(ParticipantInfo)* export kind details ([#813](https://github.com/livekit/rust-sdks/pull/813))

## [0.12.42](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.41...rust-sdks/livekit-ffi@0.12.42) - 2025-12-17

### Other

- Handle server initiated mute request ([#812](https://github.com/livekit/rust-sdks/pull/812))

## [0.12.41](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.40...rust-sdks/livekit-ffi@0.12.41) - 2025-12-04

### Added

- *(connector)* initial service impl ([#790](https://github.com/livekit/rust-sdks/pull/790))

### Fixed

- fix mute/unmute events for LocalTrack. ([#799](https://github.com/livekit/rust-sdks/pull/799))

### Other

- Add RoomEvent::TokenRefreshed ([#803](https://github.com/livekit/rust-sdks/pull/803))
- Expose desktop capturer ([#725](https://github.com/livekit/rust-sdks/pull/725))

## [0.12.40](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.39...rust-sdks/livekit-ffi@0.12.40) - 2025-11-20

### Other

- change the livekit-ffi to output a static lib for cpp SDK ([#781](https://github.com/livekit/rust-sdks/pull/781))
- perform full reconnect if resume fails ([#792](https://github.com/livekit/rust-sdks/pull/792))
- E2E RPC tests ([#769](https://github.com/livekit/rust-sdks/pull/769))
- Remove unused dependencies ([#761](https://github.com/livekit/rust-sdks/pull/761))

## [0.12.39](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.38...rust-sdks/livekit-ffi@0.12.39) - 2025-10-27

### Fixed

- fix unable to locate __arm_tpidr2_save for android ffi. ([#765](https://github.com/livekit/rust-sdks/pull/765))

### Other

- Linux hardware acceleration build fixes ([#753](https://github.com/livekit/rust-sdks/pull/753))
- Expose set video quality ([#759](https://github.com/livekit/rust-sdks/pull/759))

## [0.12.38](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.37...rust-sdks/livekit-ffi@0.12.38) - 2025-10-23

### Other

- add h265 codec support ([#762](https://github.com/livekit/rust-sdks/pull/762))

## [0.12.37](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.36...rust-sdks/livekit-ffi@0.12.37) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- Derive from_variants for FFI oneof fields ([#738](https://github.com/livekit/rust-sdks/pull/738))
- Remove participant check for data packets ([#757](https://github.com/livekit/rust-sdks/pull/757))
- clamp connection timeout and fixed the comment ([#748](https://github.com/livekit/rust-sdks/pull/748))
- put examples in root Cargo workspace ([#731](https://github.com/livekit/rust-sdks/pull/731))

## [0.12.36](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.35...rust-sdks/livekit-ffi@0.12.36) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Fixed

- fix some potential audio issues, clean up the code a bit, and suppress some warnings  ([#737](https://github.com/livekit/rust-sdks/pull/737))
- do not log 'signal client closed: "stream closed"' on disconnect ([#727](https://github.com/livekit/rust-sdks/pull/727))

### Other

- Upgrade prost, use prost-build (FFI only) ([#734](https://github.com/livekit/rust-sdks/pull/734))
- Test participant disconnect ([#732](https://github.com/livekit/rust-sdks/pull/732))
- Increase RPC max RT time to 7s ([#729](https://github.com/livekit/rust-sdks/pull/729))
- E2E audio test ([#724](https://github.com/livekit/rust-sdks/pull/724))
- bump libwebrtc libs version for webrtc-sys. ([#741](https://github.com/livekit/rust-sdks/pull/741))
- Bump reqwest to 0.12 ([#711](https://github.com/livekit/rust-sdks/pull/711))

## [0.12.35](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.34...rust-sdks/livekit-ffi@0.12.35) - 2025-10-03

### Other

- updated the following local packages: webrtc-sys, livekit

## [0.12.34](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.33...rust-sdks/livekit-ffi@0.12.34) - 2025-09-29

### Fixed

- apply original participant fields in data messages ([#709](https://github.com/livekit/rust-sdks/pull/709))

### Other

- Add send_bytes method ([#691](https://github.com/livekit/rust-sdks/pull/691))
- Implement Display and Error for RpcError ([#719](https://github.com/livekit/rust-sdks/pull/719))
- Fix intermittently failing E2E reliability test ([#718](https://github.com/livekit/rust-sdks/pull/718))
- Do not modify raw packets ([#714](https://github.com/livekit/rust-sdks/pull/714))
- Disable opus red for e2ee enabled clients ([#706](https://github.com/livekit/rust-sdks/pull/706))
- Upgrade protocol to v1.41.0 ([#703](https://github.com/livekit/rust-sdks/pull/703))
- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.12.33](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.32...rust-sdks/livekit-ffi@0.12.33) - 2025-09-09

### Other

- Add option to enable audio preconnect buffer in SDK & FFI ([#700](https://github.com/livekit/rust-sdks/pull/700))
- Optional flags for video hw codec. ([#701](https://github.com/livekit/rust-sdks/pull/701))
- Data channel reliability ([#688](https://github.com/livekit/rust-sdks/pull/688))

## [0.12.32](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.31...rust-sdks/livekit-ffi@0.12.32) - 2025-09-03

### Other

- updated the following local packages: livekit-api, livekit

## [0.12.31](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.30...rust-sdks/livekit-ffi@0.12.31) - 2025-07-31

### Other

- updated the following local packages: livekit-api, livekit
# Changelog

## [0.12.30](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.29...rust-sdks/livekit-ffi@0.12.30) - 2025-07-18

### Fixed

- fix SoxrResampler flush segv ([#678](https://github.com/livekit/rust-sdks/pull/678))

## [0.12.29](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.28...rust-sdks/livekit-ffi@0.12.29) - 2025-07-16

### Other

- remove published tracks when the room is closed ([#677](https://github.com/livekit/rust-sdks/pull/677))

## [0.12.28](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.27...rust-sdks/livekit-ffi@0.12.28) - 2025-06-23

### Fixed

- `audio_frame_ms` didn't work expectedly ([#671](https://github.com/livekit/rust-sdks/pull/671))

## [0.12.27](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-ffi@0.12.26...rust-sdks/livekit-ffi@0.12.27) - 2025-06-17

### Fixed

- *(webrtc-sys-build)* add temporary workaround to fix ci in Windows ([#665](https://github.com/livekit/rust-sdks/pull/665))
- *(webrtc-sys-build)* add error context to debug issues ([#664](https://github.com/livekit/rust-sdks/pull/664))

### Other

- Expose room updates, support MoveParticipant (protocol 15) ([#662](https://github.com/livekit/rust-sdks/pull/662))
- use path.join instead of hardcoded `/` ([#663](https://github.com/livekit/rust-sdks/pull/663))
- bump version for webrtc (fix win CI) ([#650](https://github.com/livekit/rust-sdks/pull/650))
- try to fix webrtc build for iOS/macOS. ([#646](https://github.com/livekit/rust-sdks/pull/646))
- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))

## [0.12.20] - 2025-04-08

### Added

- FFI support for the new high-level data streams API

## [0.12.18] - 2025-03-21

### Fixed

- Fix several minor issues
- Revert the RPC change, need more robust way

## [0.12.17] - 2025-03-18

### Fixed

- Move RPC handlers to room

## [0.12.16] - 2025-03-06

### Fixed

- Fixed metric report issue on audio filter where room_id is sometimes empty

## [0.12.15] - 2025-03-05

### Fixed

- Ensure RTC session continues even if audio filter initialization fails

## [0.12.14] - 2025-03-04

### Added

- APM

### Fixed

- debugging nanpa
- Fixed a packaging issue

## [0.12.13] - 2025-03-04

### Fixed

- Depends the latest version of livekit-protocol

## [0.12.12] - 2025-02-28

### Added

- Add audio filter support

## [0.12.11] - 2025-02-24

### Changed

- fixed passing disconnect reason on ParticipantDisconnected events

## [0.12.10] - 2025-02-04

### Fixed

- Fix RPC invocation race bug

## [0.12.9] - 2025-02-03

### Added

- Support for track subscription permissions

## [0.12.8] - 2025-01-23

### Changed

- Rename DataStream header properties

## [0.12.7] - 2025-01-17

### Added

- Add DataStream.Trailer support
- Add an API to set buffer_amount_low_threshold for DataChannel
- Update RoomInfo to contain buffer_amount_low_threshold for DataChannel

## [0.12.6] - 2025-01-07

### Fixed

- Automatically close audio/video stream handles when the associated room is closed

## [0.12.5] - 2025-01-04

### Added

- Fix deadlock issue in nested RPC calls.

## [0.12.4] - 2024-12-22

### Added

- bump libwebrtc to m125

## [0.12.3] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.12.72 (2026-07-23)

### Fixes

- Address typo in parsing rpc server version - #1268 (@1egoman)
- Emit black keepalive frames from NativeVideoSource instead of uninitialized memory. webrtc::I420Buffer::Create leaves the pixel planes uninitialized, so the pre-capture keepalive frames could leak recycled heap contents (often fragments of earlier frames from the same process) to subscribers as the first keyframes - #1271 (@eh-steve)
- ensure failing audio filter init doesn't degrade audio quality - #1270 (@lukasIO)
- Add NVIDIA NVENC AV1 encoding when the GPU reports AV1 encode support.

#### Route LiveKit signalling through a pluggable transport (new `livekit-net` crate).

The signalling WebSocket and the two pre-connect HTTP GETs (validate, region discovery) now go through pluggable transport traits (`WsClient` for the WebSocket, `HttpClient` for request/response) resolved from a process-global registry with independent slots — a consumer can bring only HTTP, or only WebSocket. The new `livekit-net` crate owns the WebSocket/HTTP/TLS stack behind those traits and ships native (tokio / async-std) backends. Native builds are unchanged in behavior.

**Breaking (`livekit-api`, and `livekit` via `EngineError::Signal`):**

- `SignalError::WsError` is removed — `tungstenite` is no longer part of the public API. A failed WebSocket handshake now surfaces its HTTP status as `SignalError::Client`/`Server`; transport connection and close failures surface as the new `SignalError::Connection(String)` / `SignalError::Closed` variants (previously all collapsed into `Timeout`).
- `SignalError` is now `#[non_exhaustive]`, and gains a `SignalError::TransportNotConfigured` variant — returned when no transport is registered (host/foreign builds must call `livekit_net::set_ws_client` / `set_http_client` before connecting). This is a permanent configuration error; callers must not retry.
- The signalling WebSocket/HTTP/TLS crates are no longer transitive dependencies of `livekit-api`; TLS features delegate to `livekit-net`. Existing `signal-client-tokio` / `-async` / `-dispatcher` and TLS feature names are unchanged.

## 0.12.71 (2026-07-17)

### Fixes

- Emit room EOS when the underlying LiveKit room event channel closes after a server-initiated disconnect, and ignore duplicate disconnect events during teardown.
- Don't log an expected publisher data channel close as unexpected - #1224 (@longcw)

## 0.12.70 (2026-07-14)

### Fixes

- refactor: extract data-stream logic and shared types into new `livekit-common` and `livekit-data-stream` crates (public API unchanged; types are re-exported from `livekit`)
- Use concrete type for data track manager output events

## 0.12.69 (2026-07-09)

### Fixes

- feat: auto failover APIs with LK Cloud - #1196 (@davidzhao)
- Fix for dynacast error - #1213 (@MaxHeimbrock)
- Fix malformed RTC error handling
- Handle data track SID reassignment
- introduce LiveKitAPI construct, added smoke tests - #1220 (@davidzhao)
- Turn single peerconnection off by default - #1206 (@cnderrauber)

## 0.12.68 (2026-06-30)

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

## 0.12.67 (2026-06-24)

### Fixes

- Increase room event ready timeout
- harden reconnect behaviour - #1148 (@lukasIO)

## 0.12.66 (2026-06-23)

### Features

- Rename user facing APIs for Packet Trailer to Frame Metadata.

### Fixes

- Upgrade protocol to v1.48.0

## 0.12.65 (2026-06-19)

### Fixes

- fix: escalate to full reconnect if connection failed during a resume - #1175 (@davidzhao)

## 0.12.64 (2026-06-17)

### Fixes

- Add `LK_DISABLE_NVDEC` to bypass NVIDIA NVDEC decoder registration when the environment variable is set.
- return DeviceNotFound when device is not there for set_recording_devi… - #1155 (@xianshijing-lk)

#### Add dynacast support - #1003 (@chenosaurus, @stephen-derosa)

This includes a minor breaking change for `libwebrtc`: `RtpParameters` now
contains additional RTP sender state that must be preserved when round-tripping
through `set_parameters()`.

## 0.12.63 (2026-06-09)

### Fixes

- Reject oversized data messages before they break the data channel.
- Upgrade dashmap to v6
- Add per-publication video encoder backend selection. Add a video encoder backend availability query. Remove `LIVEKIT_PREFERRED_HW_ENCODER` in favor of per-publication backend selection.

## 0.12.62 (2026-06-03)

### Fixes

- Add rpc max_round_trip_latency and move to builder pattern - #1127 (@1egoman)
- [allow(dead_code)] for dead function in room module - #1128 (@stephen-derosa)
- Send publisher offer with join request to accelerate connection - #996 (@cnderrauber)

## 0.12.61 (2026-05-29)

### Fixes

- bump protocol to v1.46.4 - #1121 (@lukasIO)

## 0.12.60 (2026-05-21)

### Features

- Introduce pipeline options for remote data tracks, support multiple in-flight frames.

#### feat: add Android application context initialization for PlatformAudio support.

Android requires `ContextUtils.initialize(applicationContext)` before WebRTC audio components can be created. This change:

- Adds `livekit_ffi_initialize_android_context()` C FFI function for Unity and other FFI consumers
- Uses `CreateAndroidAudioDeviceModule()` instead of generic `CreateAudioDeviceModule()` on Android
- Handles empty device GUIDs on Android (falls back to index 0)
- Documents Android-specific limitations: single default device, no app-level device selection

Platform notes:
- Android device enumeration returns only one "default" device with empty name/GUID
- Audio routing (speaker/earpiece/Bluetooth) is controlled by Android's AudioManager, not WebRTC

### Fixes

- Filter internal data streams out of livekit-ffi interface - #1112 (@1egoman)

## 0.12.59 (2026-05-20)

### Fixes

- Bugfix: Always emit Disconnected on engine close - #1096 (@MaxHeimbrock)
- (WIP) FFI room event ready signal after initial connection - #1068 (@ladvoc, @stephen-derosa)
- Support for large RPC messages using data streams - #1013 (@1egoman)

## 0.12.58 (2026-05-18)

### Features

- FFI logging improvements

#### Make `sample_rate` and `num_channels` optional in `NewAudioSourceRequest`.

These fields are ignored for `AudioSourcePlatform` (ADM uses hardware native settings) and for `AudioSourceNative` fast path (queue_size_ms=0, frame values used directly). Defaults to 48000 Hz and 1 channel when not specified.

### Fixes

- fix: don't fire local_track_subscribed during reconnect - #1099 (@davidzhao)
- Fix LocalTrackPublished handle leak - #1065 (@MaxHeimbrock)
- Return EOS event from data track stream read request

## 0.12.57 (2026-05-14)

### Fixes

- feat: add scalability mode for AV1/VP9. - #1076 (@cloudwebrtc)
- Add `LIVEKIT_PREFERRED_HW_ENCODER` to prefer `nvenc` or `vaapi` hardware video encoding when both are available.
- Reword audio filter logs to be less confusing - #1092 (@1egoman)

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

## 0.12.56 (2026-05-11)

### Fixes

- fix: Sync inner.enabled state for E2EE manager. - #1073 (@cloudwebrtc)
- Upgrade protocol to v1.45.8

## 0.12.55 (2026-05-11)

### Fixes

- chore: add LocalTrackRepublished event for FFI clients - #1072 (@davidzhao)

## 0.12.54 (2026-05-10)

### Features

- Bump `rustls-webpki` to 0.103.13, addressing [GHSA-82j2-j2ch-gfr8](https://github.com/advisories/GHSA-82j2-j2ch-gfr8)
- Expose error message on EOS for data track subscriptions

### Fixes

- Fix missing `libwebrtc.jar` for Android builds, harden build scripts
- fix: derive `simulcasted` from non-deprecated TrackInfo fields - #1052 (@cloudwebrtc)
- fix race in download_webrtc to reduce flaky build - #1047 (@hechen-eng)
- Improve WebRTC build scripts and add external_audio_source patch - #1053 (@xianshijing-lk)
- support SimulateScenario through FFI to improve testing - #1069 (@davidzhao)
- TEL-464: reduce redundant resampling in audio filter - #1019 (@hechen-eng)

## 0.12.53 (2026-04-23)

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

## 0.12.52 (2026-04-02)

### Features

- Initial support for data tracks

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.12.51 (2026-03-31)

### Fixes

- Expose participant active event, state, and joined at
- fix unity android build with "livekit" prefixed jni - #983 (@xianshijing-lk)
- Upgrade to thiserror 2

## 0.12.50 (2026-03-22)

### Features

#### E2EE: allow setting key_ring_size and key_derivation_algorithm, update webrtc to m144

##921 by @onestacked

This PR uses [this webrtc-sdk PR](https://github.com/webrtc-sdk/webrtc/pull/224) to configure the KDF.

I've tested this with https://codeberg.org/esoteric_programmer/matrix-jukebox and it is compatible with Element Call.

Fixed: https://github.com/livekit/rust-sdks/issues/796

### Fixes

- Fix H.264 codec matching

#### add bounded buffer to audio_stream, and use 10 frames as the default

##945 by @xianshijing-lk

#### Send client os and os_version from rust

##952 by @MaxHeimbrock

Adds [os_info](https://crates.io/crates/os_info) crate as dependency and sends the data for client connections.

## 0.12.49 (2026-03-13)

### Fixes

- Update livekit dependencies
