---
webrtc-sys: patch
---

Fix Jetson AV1 publishing for WebRTC by forcing real keyframes via V4L2_CID_MPEG_MFC51_VIDEO_FORCE_FRAME_TYPE (the AV1-oriented FORCE_IDR_FRAME/FORCE_INTRA_FRAME controls are silently ignored by the encoder, so PLI-driven keyframe requests never recovered the receiver), disabling IVF frame headers, validating low-overhead OBU output, caching and prepending sequence-header OBUs on keyframes, and treating empty MMAPI capture buffers as encode failures.
