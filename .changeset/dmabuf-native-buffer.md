---
"libwebrtc": minor
"webrtc-sys": minor
---

Add `NativeBuffer::from_dmabuf` (Linux), wrapping a DMA buffer fd as a native
video frame buffer with a release hook that fires when the WebRTC pipeline
drops its last reference, enabling zero-copy capture-to-encoder paths (e.g.
Jetson Argus into the Jetson hardware encoder) through the standard
`capture_frame` publish path. The dmabuf fd-to-surface cache is now evictable
via `remove_dmabuf_surface_cache_entry`, fixing stale-surface lookups when fd
numbers are recycled after a capture session is destroyed.
