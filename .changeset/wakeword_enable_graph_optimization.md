---
livekit-wakeword: patch
---

# Enable graph optimization for the tract backend

Sessions were built with `Session::builder()` without setting a graph optimization
level. The `ort-tract` backend used on every target except aarch64 Windows runs
tract's `into_optimized()` only when the session requests an optimization level, so
wake word inference ran the unoptimized graph. Requesting `Level3` — ONNX Runtime's
own default, so the native backend is unaffected — made `predict()` over a 2 s window
7.4x faster (534.5 ms to 72.5 ms median, Apple M-series release build).
