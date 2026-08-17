---
webrtc-sys: patch
webrtc-sys-build: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

fix: bump libwebrtc to webrtc-b9233c3-2 so TURN/TLS can use the OS trust store

WebRTC validates TURN/TLS against a small set of anchors compiled into
`rtc_base/ssl_roots.h`, generated in 2023 from Google's own PKI list. It has no
Amazon, Starfield Services or ISRG roots, so a relay-only connection to a TURN
server fronted by AWS ACM or Let's Encrypt times out with `unknown_ca` even
though the host OS trusts the chain.

This build picks up webrtc-sdk/webrtc#277, which falls back to the operating
system's trust store when the built-in anchors yield no path. It sits in
`rtc_base` below every SDK, so the C++ API that webrtc-sys binds is covered
without any Rust-side change.
