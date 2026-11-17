---
livekit-ffi: patch
---

Reuse video frame conversion buffers across frames instead of allocating and freeing a frame-sized block for every frame.
