---
livekit-signaling: minor
livekit: patch
---

The region list that a LiveKit Cloud server sends with a leave request now goes into the region cache, and a full reconnect fails over through the regions of the project URL, so the reconnect that follows a leave can fail over without fetching the list first; `livekit-signaling` adds `SignalOptions::region_url` and `SignalClient::project_url` for this.
