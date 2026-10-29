# Wake word test fixtures

Copied from [livekit-wakeword](https://github.com/livekit/livekit-wakeword), where the
Python and Swift clients test against the same files.

- `hey_livekit.onnx` — the released "hey livekit" streaming model.
- `positive.wav` — 16 kHz recording of someone saying "Hey LiveKit". Should score >= 0.5.
- `negative.wav` — 16 kHz recording without the wake word. Should score < 0.5.
- `streaming_tiny.onnx` — a small randomly initialized streaming model.
- `streaming_tiny_expected.json` — the per-80 ms scores the Python client produces
  for `streaming_tiny.onnx` on a deterministic chirp (see `chirp()` in
  `integration.rs`).
- `not_streaming.onnx` — a single Identity node with a v1-style `embeddings` input,
  used to check that non-streaming models are rejected.
