---
livekit-ffi: patch
---

Add a `kind` to `CaptureError` so clients can tell whether a capture failed in its source, was rejected by the RTC source, or ended because the pump panicked.
