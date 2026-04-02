# Changelog

## [0.7.1](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.7.0...rust-sdks/livekit-protocol@0.7.1) - 2026-02-16

### Other

- update Cargo.toml dependencies

## [0.7.0](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.6.0...rust-sdks/livekit-protocol@0.7.0) - 2026-02-09

### Other

- update proto & fix CI ([#871](https://github.com/livekit/rust-sdks/pull/871))
- add can_manage_agent_session permission ([#870](https://github.com/livekit/rust-sdks/pull/870))
- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))
- Upgrade protocol to v1.44.0 ([#857](https://github.com/livekit/rust-sdks/pull/857))

## [0.6.0](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.5.2...rust-sdks/livekit-protocol@0.6.0) - 2025-12-04

### Added

- *(connector)* initial service impl ([#790](https://github.com/livekit/rust-sdks/pull/790))

## [0.5.2](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.5.1...rust-sdks/livekit-protocol@0.5.2) - 2025-11-20

### Other

- Remove unused dependencies ([#761](https://github.com/livekit/rust-sdks/pull/761))

## [0.5.1](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.5.0...rust-sdks/livekit-protocol@0.5.1) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))

## [0.5.0](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.4.0...rust-sdks/livekit-protocol@0.5.0) - 2025-09-29

### Other

- Upgrade protocol to v1.41.0 ([#703](https://github.com/livekit/rust-sdks/pull/703))

## [0.4.0](https://github.com/livekit/rust-sdks/compare/rust-sdks/livekit-protocol@0.3.10...rust-sdks/livekit-protocol@0.4.0) - 2025-06-17

### Other

- Expose room updates, support MoveParticipant (protocol 15) ([#662](https://github.com/livekit/rust-sdks/pull/662))

## [0.3.9] - 2025-03-04

### Fixed

- Fixed the crashing issue in Promise::try_result

## [0.3.8] - 2025-02-05

### Fixed

- Fixed a dependency issue

## [0.3.7] - 2025-01-17

### Changed

- Update protocol version to v1.31.0
## 0.7.4 (2026-04-02)

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

## 0.7.3 (2026-03-31)

### Fixes

- Upgrade to thiserror 2

## 0.7.2 (2026-03-22)

### Fixes

- Add disconnectReason to Room::close
