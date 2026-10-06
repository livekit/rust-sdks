---
webrtc-sys: minor
libwebrtc: minor
livekit: minor
livekit-capture: patch
livekit-ffi: patch
---

Add `livekit::webrtc::enable_null_video_decoder()` and `PeerConnectionFactoryOptions::null_video_decoder`, a load-testing option that skips video decoding for VP8, VP9, H264 and AV1 while inbound video stats (frames decoded, freezes, resolution) stay populated.
