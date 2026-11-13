---
livekit-telemetry: patch
---

Clearing the telemetry disk cache now syncs the directory once for its deletion sweep instead of once per batch, and still syncs it when a deletion fails.
