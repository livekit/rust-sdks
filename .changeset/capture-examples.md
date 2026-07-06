---
"local_video": patch
"preencode_publish": patch
---

The `local_video` example now opens platform camera capture through `livekit-capture` (AVFoundation on macOS, V4L2 on Linux, libargus on Jetson), replacing the bundled capture code. A new `preencode_publish` example demonstrates publishing H264/H265 Annex-B TCP or RTSP streams as pre-encoded video tracks, including a GStreamer appsink encoder driven by WebRTC rate-control targets and opt-in frame metadata for capture latency measurement.
