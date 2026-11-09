---
livekit-telemetry: minor
---

Bound telemetry collector responses to 65,536 decoded bytes and retain batches with Retry-After on overflow. Document in `SPEC.md` that the exporter, not the transport, enforces the per-attempt export timeout and the 24-hour cap on server-directed pauses.
