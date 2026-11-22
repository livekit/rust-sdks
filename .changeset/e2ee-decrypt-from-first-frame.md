---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
libwebrtc: patch
webrtc-sys: patch
---

With a shared E2EE key, a subscribed track is decrypted from its first frame: recv-only transceivers get their frame cryptor when they are created, so the first keyframe no longer shows green or garbled. `KeyProvider::add_receiver`, `bind_receiver` and `remove_receiver` (libwebrtc) hold a receiver's shared-key handler and let it decrypt with a participant's handler.
