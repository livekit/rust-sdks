---
livekit-ffi: patch
---

Generate the Dart bindings with uniffi-dart e644a4ad, which frees the native scratch allocated for owned arguments and releases incoming RustBuffers after conversion, so byte and string payloads crossing the boundary no longer leak. The new revision also guards disposed Rust objects and trait wrappers against reuse and escapes dollar signs in Dart string defaults.
