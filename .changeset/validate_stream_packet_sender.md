---
livekit-data-stream: patch
livekit: patch
livekit-ffi: patch
livekit-capture: patch
livekit-rpc: patch
---

Reject incoming stream chunks and trailers that do not come from the participant whose header opened the stream. Previously these packets were matched on stream id alone, so a participant that learned an id could append content to another participant's stream, merge attributes into it, or end it. A packet from the wrong sender is dropped on its own; the stream stays open.
