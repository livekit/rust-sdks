---
livekit-ffi: minor
---

Replace `CaptureSourceEvent` with `JoinCaptureRequest`, which waits for a running capture to end and returns its result, and make disposing a capture handle wait until its capture thread exits.
