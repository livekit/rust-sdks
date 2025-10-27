# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.16](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.15...rust-sdks/webrtc-sys@0.3.16) - 2025-10-27

### Fixed

- fix unable to locate __arm_tpidr2_save for android ffi. ([#765](https://github.com/livekit/rust-sdks/pull/765))

### Other

- Linux hardware acceleration build fixes ([#753](https://github.com/livekit/rust-sdks/pull/753))

## [0.3.15](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.14...rust-sdks/webrtc-sys@0.3.15) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
- put examples in root Cargo workspace ([#731](https://github.com/livekit/rust-sdks/pull/731))

## [0.3.14](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.13...rust-sdks/webrtc-sys@0.3.14) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Fixed

- fix some potential audio issues, clean up the code a bit, and suppress some warnings  ([#737](https://github.com/livekit/rust-sdks/pull/737))
- fix linux so link issue. ([#733](https://github.com/livekit/rust-sdks/pull/733))
- change search_dirs to use cc --print-search-dirs instead of clang --print-search-dirs ([#697](https://github.com/livekit/rust-sdks/pull/697))

### Other

- bump libwebrtc libs version for webrtc-sys. ([#741](https://github.com/livekit/rust-sdks/pull/741))
- Enable buffer scaling ([#473](https://github.com/livekit/rust-sdks/pull/473))

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.12...rust-sdks/webrtc-sys@0.3.13) - 2025-10-03

### Other

- Fix empty audio frames after resample ([#722](https://github.com/livekit/rust-sdks/pull/722))

## [0.3.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.11...rust-sdks/webrtc-sys@0.3.12) - 2025-09-29

### Fixed

- fix Builds/E2E Tests CI. ([#715](https://github.com/livekit/rust-sdks/pull/715))

### Other

- nvidia codec improve ([#721](https://github.com/livekit/rust-sdks/pull/721))
- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.3.11](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.10...rust-sdks/webrtc-sys@0.3.11) - 2025-09-09

### Other

- Optional flags for video hw codec. ([#701](https://github.com/livekit/rust-sdks/pull/701))

## [0.3.10](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.9...rust-sdks/webrtc-sys@0.3.10) - 2025-09-03

### Added

- VA-API support for linux. ([#638](https://github.com/livekit/rust-sdks/pull/638))

### Fixed

- hardware rendering ([#695](https://github.com/livekit/rust-sdks/pull/695))
# Changelog

## [0.3.9](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.8...rust-sdks/webrtc-sys@0.3.9) - 2025-06-17

### Other

- updated the following local packages: webrtc-sys-build

## [0.3.8](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys@0.3.7...rust-sdks/webrtc-sys@0.3.8) - 2025-06-11

### Fixed

- fix libwebrtc.jar build issue ([#586](https://github.com/livekit/rust-sdks/pull/586))

### Other

- bump version for webrtc (fix win CI) ([#650](https://github.com/livekit/rust-sdks/pull/650))
- try to fix webrtc build for iOS/macOS. ([#646](https://github.com/livekit/rust-sdks/pull/646))
- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))
- expose apm stream_delay ([#616](https://github.com/livekit/rust-sdks/pull/616))
- Add i420_to_nv12 ([#605](https://github.com/livekit/rust-sdks/pull/605))
- ffi-v0.13.0 ([#590](https://github.com/livekit/rust-sdks/pull/590))
- add AudioProcessingModule ([#580](https://github.com/livekit/rust-sdks/pull/580))

## [0.3.7] - 2025-02-05

### Added

- Expose DataChannel.bufferedAmount property

## [0.3.6] - 2024-12-14

### Added

- bump libwebrtc to m125
