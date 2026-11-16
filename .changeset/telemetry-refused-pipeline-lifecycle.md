---
livekit-telemetry: patch
---

A telemetry pipeline that `install` refuses after the opt-out is now cancelled, so its exporter stops by itself (the caller still joins the task), and it stays known to later `disable` calls, which retry a failed delete of its caller-provided cache; installing the installed pipeline again no longer hands it back as the outgoing one.
