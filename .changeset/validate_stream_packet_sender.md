---
livekit-data-stream: major
livekit: major
livekit-ffi: major
livekit-capture: patch
livekit-rpc: patch
---

Reject incoming stream chunks and trailers that do not come from the participant whose header opened the stream. Previously these packets were matched on stream id alone, so a participant that learned an id could append content to another participant's stream, merge attributes into it, or end it. A packet from the wrong sender is dropped on its own; the stream stays open.

`StreamError` (and the FFI `DataStreamError`) gain a `ParticipantIdentityMismatch { expected, received }` variant, matching the web SDK's `DataStreamErrorReason.ParticipantIdentityMismatch`; exhaustive matches need a new arm.
