---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Load the Jetson MMAPI encoder's runtime libraries (libnvbufsurface, libv4l2/libnvv4l2) lazily via dlopen instead of linking them, so an aarch64 binary built with Jetson support also loads on non-Jetson ARM systems and falls back to other encoders there.
