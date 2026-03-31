# Changelog
## 0.1.2 (2026-03-31)

### Fixes

- Upgrade to thiserror 2

## 0.1.1 (2026-03-13)

### Features

#### Add livekit-wakeword crate with ONNX-based wake word detection

##926 by @pham-tuan-binh

### Summary
- New `livekit-wakeword` crate with a stateless wake word detection pipeline
- Pipeline: raw PCM audio → mel spectrogram → speech embeddings → classifier scores
- Mel spectrogram and embedding models are bundled at compile time via `include_bytes!`
- Wake word classifier models (e.g. `hey_livekit.onnx`) are loaded dynamically from disk at runtime
- Supports multiple classifiers simultaneously, each returning a confidence score (0-1)
- Input sample rate resampling via FIR resampler (supports 16–384 kHz, internally resamples to 16 kHz)
- Pure-Rust ONNX backend via `ort-tract` (falls back to native ONNX Runtime on aarch64-pc-windows-msvc)
- Fix mel spectrogram post-processing normalization (`x/10 + 2`) to match the openWakeWord pipeline
- Custom `WakeWordError` enum replacing `Box<dyn Error>` in the public API

### Test plan
- [x] `cargo test -p livekit-wakeword` — integration tests exercise the full pipeline
- [x] Validates score output is in [0.0, 1.0] range
- [x] Validates too-short audio returns zero scores
- [x] Positive WAV sample ("Hey LiveKit") scores >= 0.5 threshold
- [x] Negative WAV sample (non-wake-word audio) scores < 0.5 threshold
- [x] Rust scores match Python reference implementation (0.9997 positive, 0.0009 negative)
