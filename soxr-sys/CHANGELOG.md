# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/livekit/rust-sdks/compare/rust-sdks/soxr-sys@0.1.1...rust-sdks/soxr-sys@0.1.2) - 2026-02-09

### Other

- Use workspace dependencies & settings ([#856](https://github.com/livekit/rust-sdks/pull/856))

## [0.1.1](https://github.com/livekit/rust-sdks/compare/rust-sdks/soxr-sys@0.1.0...rust-sdks/soxr-sys@0.1.1) - 2025-10-22

### Other

- License check ([#746](https://github.com/livekit/rust-sdks/pull/746))
## 0.1.3 (2026-04-02)

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.
