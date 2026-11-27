---
livekit-signaling: patch
livekit: patch
---

Fix a cancelled `Room::connect` leaving its signalling connection open: dropping a `SignalClient`, or a connect that is still waiting for the server, now closes the WebSocket instead of leaving the participant in the room until the server gives up on it.
