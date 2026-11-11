---
livekit-net: patch
---

The native HTTP client no longer follows a redirect from `https` to a plaintext scheme, so an `Authorization` header never reaches a plaintext URL on the same host and port. An `http` to `https` upgrade is still followed.
