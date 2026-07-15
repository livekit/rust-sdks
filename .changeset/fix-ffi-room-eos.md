---
"livekit": patch
"livekit-ffi": patch
---

Emit room EOS when the underlying LiveKit room event channel closes after a server-initiated disconnect, and ignore duplicate disconnect events during teardown.
