---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Add native video pipeline timing instrumentation for local video measurements, exposing local publish and subscribe timing through async streams and subscriber overlay GPU upload and receive-to-GPU latency metrics through explicit timing observers. Subscriber receive timing now uses WebRTC's first-packet receive timestamp when available, with decoder-input timing clamped to preserve stage ordering across clock conversions.
