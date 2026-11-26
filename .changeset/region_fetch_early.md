---
livekit-signaling: patch
---

`SignalClient::connect` on a LiveKit Cloud URL now fetches the region list during the first connection attempt, so a region failover no longer waits for the fetch after that attempt fails.
