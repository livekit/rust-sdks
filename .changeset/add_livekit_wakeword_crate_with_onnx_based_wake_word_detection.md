---
webrtc-sys-build: minor
livekit-protocol: minor
livekit: minor
imgproc: minor
livekit-ffi: minor
yuv-sys: minor
webrtc-sys: minor
soxr-sys: minor
libwebrtc: minor
livekit-api: minor
---

# Add livekit-wakeword crate with ONNX-based wake word detection

#926 by @pham-tuan-binh

## Summary
- New `livekit-wakeword` crate with a stateless wake word detection pipeline
- Pipeline: raw PCM audio → mel spectrogram → speech embeddings → classifier scores
- Mel spectrogram and embedding models are bundled at compile time via `include_bytes!`
- Wake word classifier models (e.g. `hey_livekit.onnx`) are loaded dynamically from disk at runtime
- Supports multiple classifiers simultaneously, each returning a confidence score (0-1)

## Test plan
- [x] `cargo test -p livekit-wakeword` — single end-to-end test exercises full pipeline with `hey_livekit.onnx`
- [x] Validates score output is in [0.0, 1.0] range
- [x] Validates too-short audio returns zero scores
