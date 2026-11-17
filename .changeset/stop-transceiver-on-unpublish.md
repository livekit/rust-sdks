---
livekit: patch
---

Stop the transceiver in `LocalParticipant::unpublish_track` so libwebrtc releases the send stream, its encoder and the encoder queue thread right away instead of keeping them until the room is closed.
