---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Switching the platform audio device over FFI now works while audio is active, and a failed `switch_recording_device` / `switch_playout_device` resumes audio on the previously selected device instead of leaving it stopped.
