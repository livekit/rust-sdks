---
livekit: patch
---

Disable libwebrtc default features in workspace dependency wiring so `livekit --no-default-features -F tokio` no longer re-enables `glib-main-loop` transitively.
