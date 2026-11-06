---
livekit-telemetry: minor
---

Add `global`, the process-wide telemetry pipeline with its platform instruments and a synchronous opt-out. Release registry locks before instrument destruction so destructor capture remains safe. Retain failed opt-out erasures for retry, avoid directory inspection under registry locks, compact retired pipelines independently of disk paths, and cancel all exporters within one aggregate wait budget. Document the instrument lifecycle re-entry restrictions.
