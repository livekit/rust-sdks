# Changelog
## 0.2.0 (2026-11-06)

### Breaking Changes

#### Support LiveKit WakeWord v2 streaming models

`WakeWordModel` now runs v2 models: one stateful streaming `.onnx` file per wake word,
exported by `livekit-wakeword train`, stepped every 80 ms of 16 kHz audio. The API
matches the Python and Swift clients:

- `predict` takes PCM chunks of any size as they arrive and returns each model's
  highest score among the 80 ms steps the call completed (`0.0` if none). State
  carries over between calls; there is no longer a 2 s window to buffer.
- `predict_clip` scores a whole clip from a fresh state.
- `reset` forgets all audio heard so far, e.g. after a detection.
- `unload_model` and `model_names` manage loaded models.
- `WakeWordModel::with_session_options` takes a `SessionOptions` (graph optimization
  level, intra/inter-op threads, sequential execution, thread spinning and session
  config entries), matching the Python client's `sess_options`. It applies to every
  model, including ones a later `load_model` call adds.

`SessionOptions::default()` requests `GraphOptimizationLevel::Level3`. On the
`ort-tract` backend, used on every target except aarch64 Windows, tract optimizes the
graph only when a level is requested: the hey livekit model takes 2.0 ms per 80 ms step
with optimization and 380 ms without, which is slower than real time.

On the native ONNX Runtime backend, `mlas.disable_kleidiai` defaults to `"1"` unless
`config_entries` sets it, as in the Python client. This avoids wrong results from
ONNX Runtime 1.27's KleidiAI kernels on arm64.

### Breaking changes

- v1 classifier models are not supported. `load_model` returns
  `WakeWordError::NotStreamingModel` for them; retrain the phrase with
  `livekit-wakeword train`.
- The bundled mel spectrogram and embedding models are removed, along with the
  `MEL_BINS`, `EMBEDDING_WINDOW`, `EMBEDDING_STRIDE`, `EMBEDDING_DIM` and
  `MIN_EMBEDDINGS` constants. `FRAME_SAMPLES` (1280) is new.
- `WakeWordError` gains `NotStreamingModel` and `ModelOutputMissing`.

## 0.1.3 (2026-04-02)

### Fixes

#### use the bounded buffer for video stream

##956 by @xianshijing-lk

Before this PR, it uses an unbounded buffer for video stream, that will cause multiple problems:
1, video will be lagged behind if rendering is slow or just wake up from background
2, it will be out of sync with audio

This PRs provides options to set a bounded buffer for video stream, and use 1 buffer as the default option.

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
