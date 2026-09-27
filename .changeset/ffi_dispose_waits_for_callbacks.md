---
livekit-ffi: patch
---

`livekit_ffi_dispose` now waits for event callbacks that are in progress, so no callback runs after it returns.
