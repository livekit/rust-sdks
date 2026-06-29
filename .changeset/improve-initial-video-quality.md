---
livekit: minor
livekit-ffi: minor
libwebrtc: minor
---

Improve initial video quality by setting `x-google-start-bitrate` SDP hint for all video codecs (VP8, VP9, AV1, H264, H265) and defaulting to `MaintainResolution` degradation preference.

This addresses the issue where video starts blurry for several seconds before improving, by:
1. Telling WebRTC's bandwidth estimator to start at 70% of target bitrate instead of ramping up from ~300kbps
2. Preferring frame drops over resolution reduction when bandwidth is constrained

The `DegradationPreference` option is now exposed via FFI for Python, C++, Unity, and Node SDKs.
