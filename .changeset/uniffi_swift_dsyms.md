---
livekit-uniffi: patch
---

Embed per-slice `.dSYM` bundles in the Swift `RustLiveKitUniFFI.xcframework` again so Xcode archives and crash symbolication can resolve Rust frames (livekit/client-sdk-swift#1149).
