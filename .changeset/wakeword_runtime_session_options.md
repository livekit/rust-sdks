---
livekit-wakeword: minor
---

# Runtime-configurable ONNX session options

`WakeWordModel::with_session_options` and `load_model_with_session_options` accept a
`SessionOptions` describing how the ONNX sessions are created — graph optimization
level, intra/inter-op threads, sequential execution, thread spinning, and arbitrary
session config entries — matching the `sess_options` parameter the Python SDK takes.
The default is `SessionOptions::default()`, whose `OptimizationLevel::Level3` matches
ONNX Runtime's own default; the `ort-tract` backend used on every target except
aarch64 Windows runs tract's `into_optimized()` only when a level is requested, so
requesting one made `predict()` over a 2 s window 6.2x faster (329 ms to 53 ms
median, Apple M-series release build). Options that the active backend does not
implement are skipped rather than failing session creation.
