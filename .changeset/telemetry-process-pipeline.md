---
livekit-telemetry: minor
---

Add `global`, the process-wide telemetry pipeline with platform instruments and a synchronous opt-out. Allow capture from instrument destructors and document lifecycle re-entry restrictions. Retry failed erasures. Keep storage inspection outside registry locks. Compact retired pipelines independently of disk paths. Cancel exporters together and share their wait budget.
