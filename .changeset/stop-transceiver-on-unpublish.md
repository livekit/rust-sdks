---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Stop the transceiver in `LocalParticipant::unpublish_track` so libwebrtc releases the send stream, its encoder and the encoder queue thread right away instead of keeping them until the room is closed. The transceiver that holds the last live m-section of the publisher connection is kept, as in livekit/client-sdk-js#2282, so the connection keeps its transport for the next publish.
