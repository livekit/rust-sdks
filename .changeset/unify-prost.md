---
livekit-protocol: patch
livekit: patch
livekit-api: patch
livekit-data-stream: patch
livekit-uniffi: patch
---

Unify `prost` on the workspace version (0.14, with `pbjson`/`pbjson-types` 0.9) across
`livekit-protocol`, `livekit`, `livekit-api`, `livekit-data-stream` and `livekit-uniffi`, so
binaries that combine these crates link a single prost. The committed generated code is unchanged.
