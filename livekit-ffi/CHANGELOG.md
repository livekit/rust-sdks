# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
