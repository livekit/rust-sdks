---
"livekit-capture": minor
---

Add a `livekit-capture` crate with codec-neutral capture types, H264/H265/VP8/VP9/AV1 passthrough support, common encoded ingress helpers, TCP byte-stream encoded ingress, RTSP-over-TCP encoded ingress, GStreamer appsink encoded ingress, macOS AVFoundation decoded-frame capture, Linux V4L capture, and Jetson libargus capture hooks. Encoded sources honor WebRTC rate-control targets, validate pre-encoded AV1 and H265 access units on ingest, and support opt-in frame metadata for capture latency measurement. The capture crate reports capture-origin timing such as optional sensor timestamps, while packet-trailer frame metadata remains a publishing concern.
