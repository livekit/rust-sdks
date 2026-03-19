---
webrtc-sys: patch
webrtc-sys-build: patch
libwebrtc: patch
---

# fix clang build issue from zed patches (#949)

#950 by @cloudwebrtc

* webrtc-sys: Use clang instead of gcc

* Debug CI output for aarch64-linux

* ci: Install lld for aarch64-linux FFI builders

* webrtc-sys: Disable CREL
