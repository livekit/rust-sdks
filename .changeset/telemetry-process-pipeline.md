---
livekit-telemetry: minor
---

Add `global`, the process-wide telemetry pipeline with its platform instruments and a synchronous opt-out. Release registry locks before instrument destruction so destructor capture remains safe.
