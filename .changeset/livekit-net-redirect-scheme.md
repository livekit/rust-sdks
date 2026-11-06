---
livekit-net: patch
---

The native HTTP client no longer follows a redirect that changes the scheme, so an `Authorization` header never reaches a plaintext URL on the same host and port.
