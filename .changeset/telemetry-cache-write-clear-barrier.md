---
"livekit-telemetry": patch
---

Serialize cache writes and clears across process instances and reject replacements whose parent is already missing without resurrecting evicted batches.
