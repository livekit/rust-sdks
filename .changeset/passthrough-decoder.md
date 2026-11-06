---
webrtc-sys: minor
libwebrtc: minor
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Add pass-through video decoding: a `PeerConnectionFactory` created with `VideoDecoderBackend::PassThrough` delivers received video as encoded access units through `NativeEncodedVideoStream`, without dropping frames, and `RtcVideoTrack::request_keyframe` asks a remote sender for a keyframe.
