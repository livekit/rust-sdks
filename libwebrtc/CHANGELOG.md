# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.26](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.25...rust-sdks/libwebrtc@0.3.26) - 2026-02-16

### Other

- add is_screencast to VideoSource ([#896](https://github.com/livekit/rust-sdks/pull/896))

## [0.3.25](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.24...rust-sdks/libwebrtc@0.3.25) - 2026-02-09

### Fixed

- fix the 440->441 samples issue and pass a noop callback for release ([#848](https://github.com/livekit/rust-sdks/pull/848))

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- allow apm >=10ms frames ([#843](https://github.com/livekit/rust-sdks/pull/843))

## [0.3.24](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.23...rust-sdks/libwebrtc@0.3.24) - 2026-01-15

### Other

- updated the following local packages: webrtc-sys

## [0.3.23](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.22...rust-sdks/libwebrtc@0.3.23) - 2025-12-19

### Fixed

- Exclude the desktop-capturer module link for mobile. ([#817](https://github.com/livekit/rust-sdks/pull/817))

## [0.3.22](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.21...rust-sdks/libwebrtc@0.3.22) - 2025-12-17

### Other

- Expose WebRTC's audio_mixer ([#806](https://github.com/livekit/rust-sdks/pull/806))

## [0.3.21](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.20...rust-sdks/libwebrtc@0.3.21) - 2025-12-04

### Other

- move starting/stopping GLib event loop into libwebrtc crate ([#798](https://github.com/livekit/rust-sdks/pull/798))
- Expose desktop capturer ([#725](https://github.com/livekit/rust-sdks/pull/725))

## [0.3.20](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.19...rust-sdks/libwebrtc@0.3.20) - 2025-11-20

### Other

- Fix the fast path in capture_frame function, without buffering ([#778](https://github.com/livekit/rust-sdks/pull/778))

## [0.3.19](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.18...rust-sdks/libwebrtc@0.3.19) - 2025-10-27

### Other

- updated the following local packages: webrtc-sys

## [0.3.18](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.17...rust-sdks/libwebrtc@0.3.18) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))

## [0.3.17](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.16...rust-sdks/libwebrtc@0.3.17) - 2025-10-13

### Added

- *(e2ee)* add data channel encryption ([#708](https://github.com/livekit/rust-sdks/pull/708))

### Other

- Enable buffer scaling ([#473](https://github.com/livekit/rust-sdks/pull/473))

## [0.3.16](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.15...rust-sdks/libwebrtc@0.3.16) - 2025-10-03

### Other

- updated the following local packages: webrtc-sys

## [0.3.15](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.14...rust-sdks/libwebrtc@0.3.15) - 2025-09-29

### Fixed

- fix Builds/E2E Tests CI. ([#715](https://github.com/livekit/rust-sdks/pull/715))

## [0.3.14](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.13...rust-sdks/libwebrtc@0.3.14) - 2025-09-09

### Other

- updated the following local packages: webrtc-sys

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.12...rust-sdks/libwebrtc@0.3.13) - 2025-09-03

### Other

- updated the following local packages: webrtc-sys
# Changelog

## [0.3.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.11...rust-sdks/libwebrtc@0.3.12) - 2025-06-17

### Other

- updated the following local packages: livekit-protocol, webrtc-sys

## [0.3.11](https://github.com/livekit/rust-sdks/compare/rust-sdks/libwebrtc@0.3.10...rust-sdks/libwebrtc@0.3.11) - 2025-06-11

### Fixed

- fix uint32 overflow ([#615](https://github.com/livekit/rust-sdks/pull/615))

### Other

- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))
- expose apm stream_delay ([#616](https://github.com/livekit/rust-sdks/pull/616))
- Add i420_to_nv12 ([#605](https://github.com/livekit/rust-sdks/pull/605))
- ffi-v0.13.0 ([#590](https://github.com/livekit/rust-sdks/pull/590))
- add AudioProcessingModule ([#580](https://github.com/livekit/rust-sdks/pull/580))

## [0.3.10] - 2025-02-05

### Fixed

- Fix build issue

## [0.3.9] - 2025-01-17

### Added

- Expose DataChannel.bufferedAmount property

## [0.3.8] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.3.29 (2026-04-02)

### Features

#### chore: upgrade libwebrtc to m144.

##965 by @cloudwebrtc

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.3.28 (2026-03-31)

### Fixes

- Upgrade to thiserror 2

#### fix: fix unavailable sem symbol for Linux aarch64.

##975 by @cloudwebrtc

## 0.3.27 (2026-03-22)

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

#### fix clang build issue from zed patches (#949)

##950 by @cloudwebrtc

* webrtc-sys: Use clang instead of gcc

* Debug CI output for aarch64-linux

* ci: Install lld for aarch64-linux FFI builders

* webrtc-sys: Disable CREL
