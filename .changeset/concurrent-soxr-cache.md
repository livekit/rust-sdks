---
soxr-sys: patch
livekit-ffi: patch
---

Fix concurrent construction and processing of independent fixed-rate audio resamplers by synchronizing the shared FFT caches even when OpenMP is disabled.
