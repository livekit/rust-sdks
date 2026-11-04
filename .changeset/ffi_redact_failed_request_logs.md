---
livekit-ffi: patch
---

A failed FFI request no longer writes the full request to the error log or to the Node error message, so E2EE keys in `SetSharedKey` and `SetKey` requests stay out of logs.
