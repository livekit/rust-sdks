---
libwebrtc: patch
livekit: patch
livekit-ffi: patch
webrtc-sys: patch
---

Fix CUDA and FFI resource cleanup during SDK shutdown.

NVIDIA encoder and decoder factories now share a reference-counted CUDA context
and destroy it when the final factory is dropped. FFI shutdown now releases
leftover handles one at a time so nested `drop_handle` calls do not re-enter
`DashMap::clear()`. Adds regression coverage for FFI-handle, watcher, and
configuration cleanup during disposal.
