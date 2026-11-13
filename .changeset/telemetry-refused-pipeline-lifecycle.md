---
livekit-telemetry: patch
---

A telemetry pipeline refused after the opt-out is now cancelled and stays known to later `disable` calls, which retry a failed delete of its caller-provided cache, and installing the installed pipeline again no longer hands it back as the outgoing one.
