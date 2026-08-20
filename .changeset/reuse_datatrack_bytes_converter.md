---
livekit-uniffi: patch
---

Reuse `livekit-datatrack`'s `Bytes` converter instead of registering a second one, so UniFFI emits it once and the post-generation Swift workaround is no longer needed
