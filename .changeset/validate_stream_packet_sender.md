---
livekit-data-stream: patch
---

Reject incoming stream chunks and trailers that do not come from the participant whose header opened the stream. Previously these packets were matched on stream id alone, so a participant that learned an id could append content to another participant's stream, merge attributes into it, or end it.
