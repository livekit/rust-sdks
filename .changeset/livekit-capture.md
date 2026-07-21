---
"livekit-capture": minor
---

Add a `livekit-capture` crate with codec-neutral encoded capture types, H264/H265/VP8/VP9/AV1 passthrough support, common encoded ingress helpers, and GStreamer appsink encoded ingress. Encoded sources honor WebRTC rate-control targets, validate pre-encoded AV1 and H265 access units on ingest, and support opt-in frame metadata for capture latency measurement.
