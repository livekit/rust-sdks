---
livekit-telemetry: patch
---

A telemetry shutdown now encodes the records and spans that a retained scope captures during its final drain, in bounded turns that start only before the original export deadline, instead of leaving them neither sent nor cached.
