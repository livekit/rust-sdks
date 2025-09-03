# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
