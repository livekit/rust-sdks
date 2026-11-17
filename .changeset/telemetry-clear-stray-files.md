---
livekit-telemetry: patch
---

Clearing the telemetry disk cache, as the opt-out does, now also deletes stray files in the storage directory and reports an entry it cannot delete, so the opt-out no longer reports success while a stray file stays.
