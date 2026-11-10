---
livekit-wakeword: major
---

# Support LiveKit WakeWord v2 streaming models

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

## Breaking changes

- v1 classifier models are not supported. `load_model` returns
  `WakeWordError::NotStreamingModel` for them; retrain the phrase with
  `livekit-wakeword train`.
- The bundled mel spectrogram and embedding models are removed, along with the
  `MEL_BINS`, `EMBEDDING_WINDOW`, `EMBEDDING_STRIDE`, `EMBEDDING_DIM` and
  `MIN_EMBEDDINGS` constants. `FRAME_SAMPLES` (1280) is new.
- `WakeWordError` gains `NotStreamingModel` and `ModelOutputMissing`.
