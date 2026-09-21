---
livekit: patch
livekit-data-stream: patch
livekit-ffi: patch
---

Fix reliable data channel replay: keep the full retry buffer across resumes, drop duplicate reliable packets, and ignore replayed chunks on uncompressed streams instead of failing with `MissedChunk`.
