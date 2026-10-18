---
livekit-protocol: patch
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-common: patch
livekit-data-stream: patch
livekit-datatrack: patch
livekit-ffi: patch
livekit-rpc: patch
livekit-signaling: patch
livekit-token: patch
livekit-uniffi: patch
---

`livekit-protocol` exposes the conversation schema of a thread's database, as protocol's Go package
does: `agent::THREAD_SCHEMA_VERSION` and `agent::THREAD_SCHEMA_V1` (the SQL); the crates that
depend on it follow its bump.
