---
livekit: patch
livekit-ffi: patch
---

A received transcription with an invalid track sid is now dropped with a warning. Before, it made the client panic, which aborts the process in release builds.
