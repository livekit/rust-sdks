# LiveKit Wake Word

Rust runtime for [LiveKit WakeWord](https://github.com/livekit/livekit-wakeword) models. It
detects wake words (e.g. "hey livekit") locally in streaming PCM audio, with no server round
trip. Its API matches the Python and Swift clients.

## How it works

Each wake word is a single streaming `.onnx` model exported by `livekit-wakeword train`. The
model takes 80 ms of 16 kHz audio per step and keeps its state between steps, so audio can be
fed as it arrives in chunks of any size. Each step returns a confidence score between 0 and 1.

Models are loaded from disk at runtime, so you can add or swap wake words without recompiling.
Models from the deprecated v1 implementation (openWakeWord-style classifiers) are not supported:
[retrain the phrase](https://github.com/livekit/livekit-wakeword#train-a-wake-word) to upgrade.

## Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
livekit-wakeword = "0.2"
```

Detect a wake word:

```rust
use livekit_wakeword::WakeWordModel;

// Load one or more models, specifying the input sample rate.
let mut model = WakeWordModel::new(&["path/to/hey_livekit.onnx"], 48000)?;

// Feed i16 PCM as it arrives, in chunks of any size. Resampling to 16 kHz is
// handled internally.
let scores = model.predict(&audio_chunk)?;
if scores["hey_livekit"] > 0.5 {
    println!("Wake word detected!");
    model.reset()?; // start fresh after a detection
}

// Highest score in a whole clip, starting from a fresh state.
let scores = model.predict_clip(&clip)?;
```

`predict()` returns one score per loaded model, keyed by file stem. The score is the highest
among the 80 ms steps that call completed, or `0.0` if the call completed none. The default
threshold of 0.5 is calibrated to about 0.5 false alarms per hour.

Load and unload models at runtime:

```rust
model.load_model("path/to/another.onnx", Some("custom_wakeword"))?;
model.unload_model("custom_wakeword");
```

### Session options

`WakeWordModel::with_session_options` configures the ONNX sessions it creates, like
`sess_options` in the Python client. For example, keep detection on one thread when it runs in
the background on a small machine:

```rust
use livekit_wakeword::{SessionOptions, WakeWordModel};

let options = SessionOptions {
    intra_threads: Some(1),
    inter_threads: Some(1),
    parallel_execution: Some(false),
    intra_op_spinning: Some(false),
    inter_op_spinning: Some(false),
    ..Default::default()
};
let mut model = WakeWordModel::with_session_options(&["hey_livekit.onnx"], 16000, options)?;
```

### ONNX backend

By default the crate uses [`ort-tract`](https://crates.io/crates/ort-tract) (pure-Rust ONNX
inference), so no native libraries are needed. On `aarch64-pc-windows-msvc`, where tract cannot
compile due to MSVC-incompatible assembly, the crate automatically falls back to native ONNX
Runtime. This is handled at build time, with no feature flags or configuration.

On tract, only `SessionOptions::optimization_level` takes effect; the other options are skipped.
Leave it at the default (`Level3`): with `Disable`, tract does not optimize the graph, and each
80 ms step takes longer than 80 ms to run.

## Audio requirements

| Parameter | Value |
|-----------|-------|
| Sample rate | 16,000 / 22,050 / 32,000 / 44,100 / 48,000 / 88,200 / 96,000 / 176,400 / 192,000 / 384,000 Hz |
| Format | `i16` PCM, mono |
| Chunk size | Any; each 1,280 samples at 16 kHz (80 ms) is one model step |

Pass the input sample rate to `WakeWordModel::new()`. Audio at any other rate than 16 kHz is
resampled internally.

## Models

Ready-made models such as `hey_livekit.onnx` are attached to
[LiveKit WakeWord releases](https://github.com/livekit/livekit-wakeword/releases/latest). To
train a model for your own phrase, see
[Train a wake word](https://github.com/livekit/livekit-wakeword#train-a-wake-word). Any `.onnx`
model it exports can be loaded directly by this crate.
