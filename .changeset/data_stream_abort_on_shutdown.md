---
livekit-data-stream: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
livekit-rpc: patch
livekit-uniffi: patch
---

In-flight incoming data streams now error with `AbnormalEnd` when the room closes. Before, the stream readers ended cleanly, so `read_all()` returned truncated content as if it were complete.
