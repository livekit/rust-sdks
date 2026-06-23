# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.18](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.17...rust-sdks/imgproc@0.3.18) - 2026-02-10

### Other

- updated the following local packages: yuv-sys

## [0.3.17](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.16...rust-sdks/imgproc@0.3.17) - 2026-02-09

### Other

- updated the following local packages: yuv-sys

## [0.3.16](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.15...rust-sdks/imgproc@0.3.16) - 2026-02-09

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))

## [0.3.15](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.14...rust-sdks/imgproc@0.3.15) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))

## [0.3.14](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.13...rust-sdks/imgproc@0.3.14) - 2025-09-29

### Other

- updated the following local packages: yuv-sys

## [0.3.13](https://github.com/livekit/rust-sdks/compare/rust-sdks/imgproc@0.3.12...rust-sdks/imgproc@0.3.13) - 2025-09-09

### Other

- updated the following local packages: yuv-sys
# Changelog

## [0.3.12] - 2024-12-14

### Added

- move imgproc to main rust-sdks monorepo
## 0.3.19 (2026-04-02)

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.
