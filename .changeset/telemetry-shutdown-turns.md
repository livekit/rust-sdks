---
livekit-telemetry: patch
---

A telemetry shutdown now gives the records and spans that a retained scope captures during its final drain further bounded encoding turns, which start only before the original export deadline; a started turn may finish after that deadline, and what is still queued when no further turn may start is not cached.
