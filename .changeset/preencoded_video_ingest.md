---
webrtc-sys: minor
libwebrtc: minor
livekit: minor
---

Add a pre-encoded video pipeline (`EncodedVideoTrackSource` + `PassthroughVideoEncoder`) so callers can publish already-encoded H.264/H.265/VP8/VP9/AV1 bitstreams without re-encoding, plus a new `examples/preencoded_ingest` app that ingests an Annex-B H.264 stream from a TCP server (e.g. gstreamer's `tcpserversink`).
