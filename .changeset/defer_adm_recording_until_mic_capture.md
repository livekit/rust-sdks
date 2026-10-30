---
livekit: patch
---

Defer ADM recording until mic capture starts, so the platform audio device is not opened before the local track needs it.
