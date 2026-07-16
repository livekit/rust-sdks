---
livekit: patch
---

Simplify x-google-start-bitrate logic and update degradation preference defaults

- Start bitrate: use min(90% of target, 1 Mbps) instead of adaptive network hints
- Remove slow connection detection and network quality hints on reconnect
- Default degradation preference by track source:
  - Camera: MaintainFramerate (smoother video)
  - Screenshare: MaintainResolution (clarity for text/UI)
  - Other: Balanced
