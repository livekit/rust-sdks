---
"livekit-capture": minor
"livekit": patch
"libwebrtc": patch
"webrtc-sys": patch
---

Add a `livekit-capture` crate with codec-neutral capture types, H264/H265 passthrough support, common encoded ingress helpers, TCP byte-stream encoded ingress, RTSP-over-TCP encoded ingress, GStreamer appsink encoded ingress, macOS AVFoundation decoded-frame capture, Linux V4L capture, and Jetson libargus capture hooks. The `local_video` examples now open platform camera capture through `livekit-capture` instead of depending on Nokhwa directly, and a `preencode_publish` example demonstrates publishing H264/H265 Annex-B TCP or RTSP streams as pre-encoded video tracks.
