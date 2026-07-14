---
"webrtc-sys": minor
"libwebrtc": minor
"livekit": minor
---

Add a pre-encoded video publish path: a passthrough video encoder and encoded video frame buffer in webrtc-sys, and `EncodedVideoFrame`/`EncodedVideoCodec`/`EncodedFrameType` publish APIs with a `VideoEncoderBackend::PreEncoded` backend in libwebrtc. WebRTC rate-control targets and keyframe requests are forwarded to encoded sources, and pre-encoded AV1 and H265 access units are validated on ingest.
