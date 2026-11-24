---
libwebrtc: minor
webrtc-sys: minor
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Wayland screen capture can opt in to a persistent ScreenCast portal restore token (DesktopCapturerOptions::set_persist_restore_token, DesktopCapturer::set_restore_token / restore_token), so the portal can skip its dialog next time.
