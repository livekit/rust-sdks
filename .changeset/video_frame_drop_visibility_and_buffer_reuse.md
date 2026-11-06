---
libwebrtc: patch
livekit-ffi: patch
---

Report dropped video frames to the application via `VideoFrameReceived.dropped_frames`, keep libwebrtc log messages at their own severity instead of flattening every one to debug, and reuse video frame conversion buffers rather than allocating one per frame.
