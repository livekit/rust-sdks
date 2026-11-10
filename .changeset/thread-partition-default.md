---
livekit-protocol: patch
livekit-api: patch
---

Update the v1 thread schema to explicit partition keys and expose DEFAULT_PARTITION_KEY
for framework default resolution. Keep the Go/Rust constant and schema versions aligned.
Regenerate protocol bindings and preserve existing egress requests when the protocol
adds optional stream destinations.
