---
livekit-ffi: patch
---

Clear remaining FFI handles during dispose so platform audio resources are released across repeated initialize/shutdown cycles.
