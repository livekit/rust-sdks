---
livekit-telemetry: patch
---

A telemetry shutdown again encodes the records and spans that a retained scope captures during its final drain, in bounded turns until the original export deadline, instead of leaving them neither sent nor cached.
