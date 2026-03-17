---
yuv-sys: patch
libwebrtc: patch
livekit-ffi: patch
livekit-wakeword: patch
livekit-protocol: patch
soxr-sys: patch
imgproc: patch
webrtc-sys-build: patch
webrtc-sys: patch
livekit: patch
livekit-api: patch
---

# End-to-end testing for video streams

#937 by @ladvoc

Adds end-to-end tests to verify that video streaming works properly, parameterized by codec, resolution, and whether simulcast is enabled.

## Methodology

Frames are published with a known, uniform luminance value across the frame, which the subscriber verifies within a certain margin of error. The subscriber also verifies the received frame’s aspect ratio (not exact resolution as it may be smaller due to simulcast). If no frames are received (for example, due to an encoder or decoder not being created in _libwebrtc_), the test will timeout and fail.

Note: there are probably more robust ways to verify that received frames are correct, but I opted for this method for simplicity. Open to any suggestion here.

## Limitations

Currently, only VP8 and VP9 are tested in CI, since I could not get hardware encoding/decoding to work properly in the GitHub Actions runners. In the future, we may consider using self-hosted runners to ensure these paths are covered by automated testing as well.

## Other Changes

Updates the CI configuration to build tests for _aarch64_ on _macos-latest_. Previously, tests were built for _x86_64_ even though this runner is ARM-based, which required Rosetta 2 emulation and slowed down test execution.
