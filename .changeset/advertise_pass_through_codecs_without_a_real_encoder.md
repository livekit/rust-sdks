---
webrtc-sys: patch
libwebrtc: minor
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
---

Pre-encoded video publishing no longer requires a real encoder for the codec on the publishing host, so pre-encoded H265 now works on Linux arm64 builds without the Jetson encoder, and `VideoEncoderBackend::supported_codecs` reports which codecs each backend can produce.
