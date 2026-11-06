---
livekit-data-stream: patch
livekit-rpc: patch
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
---

Reject incoming stream packets from other participants, enforce decompression limits before buffering output, and prevent received files from overwriting existing files or following destination symlinks.
