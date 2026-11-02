---
livekit-signaling: patch
livekit: patch
livekit-ffi: patch
---

Add `SignalOptions::cancellation_token`, which ends a signal session for good: a connect or resume in flight stops with `SignalError::Cancelled`, and the signal task closes the transport.
