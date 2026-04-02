# Changelog

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.12...rust-sdks/webrtc-sys-build@0.3.13) - 2026-02-09

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Use the correct download url in webrtc-sys build. ([#825](https://github.com/livekit/rust-sdks/pull/825))

## [0.3.12](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.11...rust-sdks/webrtc-sys-build@0.3.12) - 2025-12-04

### Other

- Expose desktop capturer ([#725](https://github.com/livekit/rust-sdks/pull/725))

## [0.3.11](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.10...rust-sdks/webrtc-sys-build@0.3.11) - 2025-10-27

### Fixed

- fix unable to locate __arm_tpidr2_save for android ffi. ([#765](https://github.com/livekit/rust-sdks/pull/765))

## [0.3.10](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.9...rust-sdks/webrtc-sys-build@0.3.10) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))

## [0.3.9](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.8...rust-sdks/webrtc-sys-build@0.3.9) - 2025-10-13

### Other

- bump libwebrtc libs version for webrtc-sys. ([#741](https://github.com/livekit/rust-sdks/pull/741))
- Bump reqwest to 0.12 ([#711](https://github.com/livekit/rust-sdks/pull/711))

## [0.3.8](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.7...rust-sdks/webrtc-sys-build@0.3.8) - 2025-09-29

### Other

- Upgrade libwebrtc to m137. ([#696](https://github.com/livekit/rust-sdks/pull/696))

## [0.3.7](https://github.com/livekit/rust-sdks/compare/rust-sdks/webrtc-sys-build@0.3.6...rust-sdks/webrtc-sys-build@0.3.7) - 2025-06-17

### Fixed

- *(webrtc-sys-build)* add temporary workaround to fix ci in Windows ([#665](https://github.com/livekit/rust-sdks/pull/665))
- *(webrtc-sys-build)* add error context to debug issues ([#664](https://github.com/livekit/rust-sdks/pull/664))

### Other

- use path.join instead of hardcoded `/` ([#663](https://github.com/livekit/rust-sdks/pull/663))
- bump version for webrtc (fix win CI) ([#650](https://github.com/livekit/rust-sdks/pull/650))
- try to fix webrtc build for iOS/macOS. ([#646](https://github.com/livekit/rust-sdks/pull/646))
- remove ([#633](https://github.com/livekit/rust-sdks/pull/633))

## [0.3.6] - 2024-12-14

### Added

- bump libwebrtc to m125
## 0.3.15 (2026-04-02)

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.3.14 (2026-03-22)

### Fixes

#### fix: Bump webrtc build to fix build for Android JNI prefixed.

##954 by @cloudwebrtc

#### fix clang build issue from zed patches (#949)

##950 by @cloudwebrtc

* webrtc-sys: Use clang instead of gcc

* Debug CI output for aarch64-linux

* ci: Install lld for aarch64-linux FFI builders

* webrtc-sys: Disable CREL
