---
livekit-wakeword: minor
---

# Runtime-configurable ONNX session options

`WakeWordModel::with_session_options` takes a `SessionOptions` describing how the
crate creates its ONNX sessions — graph optimization level, intra/inter-op threads,
sequential execution, thread spinning, and arbitrary session config entries —
matching the `sess_options` parameter the Python SDK takes. It applies to the two
bundled feature extraction models and to every wake word classifier, including ones
a later `load_model` call adds.

`WakeWordModel::new` keeps its signature and uses `SessionOptions::default()`, whose
`GraphOptimizationLevel::Level3` matches ONNX Runtime's own default. That default
matters: the `ort-tract` backend used on every target except aarch64 Windows runs
tract's `into_optimized()` only when a level is requested, so requesting one made
`predict()` over a 2 s window 6.2x faster (329 ms to 53 ms median, Apple M-series
release build). tract implements no other session option, so the remaining fields
are skipped there rather than failing session creation.
