---
livekit-uniffi: patch
livekit-datatrack: patch
livekit-common: patch
---

Register the `Bytes` UniFFI custom type once in `livekit-common` and borrow it from each component with `uniffi::use_remote_type!`, so the converter is emitted once and the post-generation Swift workaround is no longer needed
