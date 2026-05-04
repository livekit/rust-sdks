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
## 0.12.54 (2026-05-04)

### Fixes

- fix: derive `simulcasted` from non-deprecated TrackInfo fields - #1052 (@cloudwebrtc)
- fix race in download_webrtc to reduce flaky build - #1047 (@hechen-eng)
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
