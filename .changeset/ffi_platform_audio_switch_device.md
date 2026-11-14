---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Switching the platform audio device over FFI now works while audio is active, and `switch_recording_device` / `switch_playout_device` no longer leave audio stopped when the switch fails.
