---
livekit-telemetry: minor
---

Rename the public `RtcStatsSample.track_sid` field and constructor parameter to `track_id`; callers and generated bindings must migrate to the new name while keeping the same LiveKit track identifier values.
