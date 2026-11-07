---
libwebrtc: patch
livekit-ffi: patch
---

Report frames a video stream decoded but never delivered via the new `VideoFrameReceived.dropped_frames`, which no WebRTC stat covers.
