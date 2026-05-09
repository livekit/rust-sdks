---
local_video: patch
webrtc-sys: patch
---

Use the screen capture callback time for local video packet trailer timestamps, avoid an extra I420 copy in the screen publisher path, and add a macOS native ScreenCaptureKit screen capture path.
