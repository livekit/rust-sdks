# Changelog

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
