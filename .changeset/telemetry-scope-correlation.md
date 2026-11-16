---
livekit-telemetry: patch
---

Keep telemetry correlation consistent: a room identity change closes the open RTC windows and applies all four fields together, scoped records and spans no longer link to a span of another session, and `max_cache_bytes` gets its documented one-byte floor.
