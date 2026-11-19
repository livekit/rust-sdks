---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Treat a negative device count from the audio device module as zero devices in `PlatformAudio`, so `recording_devices()` and `playout_devices()` no longer iterate over `usize::MAX` indices when enumeration fails.
