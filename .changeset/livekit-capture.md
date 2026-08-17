---
"livekit-capture": minor
---

Add a `livekit-capture` crate with codec-neutral encoded capture types, H264/H265/VP8/VP9/AV1 passthrough support, common encoded ingress helpers, and GStreamer appsink encoded ingress. Encoded sources honor WebRTC rate-control targets, validate pre-encoded AV1 and H265 access units on ingest, and support opt-in frame metadata for capture latency measurement.

Add a `source-device-argus` feature that captures NVIDIA Jetson CSI sensors through libargus as an additional Linux device-source backend: hardware-ISP NV12 frames published as zero-copy DMA buffers. Argus sensors are enumerated alongside V4L2 devices under `argus:N` identifiers, their raw Bayer V4L2 nodes are suppressed, and on Jetson the default device (and index order) prefers the CSI sensor — USB webcams keep using V4L2. The feature is inert off Jetson.
