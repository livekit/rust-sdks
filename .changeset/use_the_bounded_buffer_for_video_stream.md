---
libwebrtc: patch
imgproc: patch
livekit: patch
yuv-sys: patch
soxr-sys: patch
livekit-protocol: patch
livekit-ffi: patch
livekit-wakeword: patch
webrtc-sys: patch
livekit-api: patch
webrtc-sys-build: patch
---

# use the bounded buffer for video stream

#956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.
