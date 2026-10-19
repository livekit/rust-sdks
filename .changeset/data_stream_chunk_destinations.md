---
livekit-data-stream: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
livekit-rpc: patch
livekit-uniffi: patch
---

Outgoing data stream chunk and trailer packets now carry the stream's destination identities. Before, only the header was targeted, so the content of a stream sent to specific participants was delivered to everyone in the room.
