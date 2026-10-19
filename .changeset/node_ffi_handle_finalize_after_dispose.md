---
livekit-ffi: patch
---

Stop the Node bindings' `FfiHandle` finalizer from throwing when garbage collection runs after `livekitDispose()`, and treat handles already released by shutdown as disposed.

