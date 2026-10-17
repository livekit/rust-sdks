---
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
livekit-signaling: patch
---
Retain failed and unattempted queueable signals in FIFO order until a transport acknowledges them, preserving acknowledgements across caller cancellation so successful writes are not replayed on the same connection or after reconnecting.
