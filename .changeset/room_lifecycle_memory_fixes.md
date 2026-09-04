---
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fix room-session and data-channel leaks across connect/disconnect cycles.

The E2EE manager callback now captures `RoomSession` weakly so the session can
drop after disconnect. Data-channel observer callbacks are cleared during RTC
teardown so the observer/callback cycle cannot keep peer connections alive.
Adds regression coverage for room-session destruction and data-channel callback
cleanup.
