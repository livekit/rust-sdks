---
webrtc-sys: patch
libwebrtc: minor
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
livekit-capture: patch
---

Pre-encoded video publishing no longer requires a local encoder for its codec, so pre-encoded H265 works on hosts without an H265 encoder, a camera track requesting a codec no local encoder produces now publishes VP8 instead of a codec it cannot encode, and `PeerConnectionFactory::video_encoder_codecs` reports which codecs each encoder backend can produce.
