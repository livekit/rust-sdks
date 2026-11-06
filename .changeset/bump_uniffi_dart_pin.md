---
livekit-ffi: patch
---

# Dart bindings: free native buffers after conversion

The Dart bindings are now generated with uniffi-dart at e644a4ad, which frees the native scratch allocated when lowering owned arguments (uniffi-dart #180) and releases incoming RustBuffers once their contents are copied out (uniffi-dart #182). Before this, every call that crossed the boundary with a byte or string payload leaked that payload, so a data stream round trip of 8 MiB grew the process by about the same amount for good.

The new revision also guards disposed Rust objects and trait wrappers against reuse (uniffi-dart #172) and escapes dollar signs in Dart string defaults (uniffi-dart #176).
