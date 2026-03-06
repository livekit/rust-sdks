# LiveKit Wake Word

Rust inference engine for wake word detection, powered by ONNX Runtime. This crate runs classifier models trained with [livekit-wakeword](https://github.com/livekit/livekit-wakeword) to detect wake words (e.g. "hey livekit") from raw PCM audio.

## How it works

Audio is processed through a three-stage ML pipeline:

1. **Mel Spectrogram** - Converts raw 16 kHz PCM audio into a mel-frequency spectrogram (32 bins)
2. **Embedding** - Extracts 96-dimensional speech embeddings from the spectrogram using a sliding window
3. **Classification** - Runs one or more classifier models on the embeddings to produce per-wake-word confidence scores

The mel spectrogram and embedding models are embedded in the binary at compile time. Classifier models (`.onnx` files) are trained using the [livekit-wakeword](https://github.com/livekit/livekit-wakeword) Python toolkit and loaded from disk at runtime, making it easy to add or swap wake words without recompiling.

## Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
livekit-wakeword = "0.1.0"
```

### ONNX backend

By default the crate uses [`ort-tract`](https://crates.io/crates/ort-tract) (pure-Rust ONNX inference), so no native libraries are needed. On `aarch64-pc-windows-msvc`, where tract cannot compile due to MSVC-incompatible assembly, the crate automatically falls back to native ONNX Runtime. This is handled at build time — no feature flags or configuration required.

Detect a wake word:

```rust
use livekit_wakeword::wakeword::WakeWordModel;

// Load one or more classifier ONNX models, specifying the input sample rate
let mut model = WakeWordModel::new(&["path/to/hey_livekit.onnx"], 48000)?;

// Feed i16 PCM audio — resampling to 16 kHz is handled internally
let predictions = model.predict(&audio)?;

for (name, score) in &predictions {
    println!("{name}: {score:.4}");
}
```

You can load additional classifiers at runtime:

```rust
model.load_model("path/to/another.onnx", "custom_wakeword")?;
```

## Audio requirements

| Parameter | Value |
|-----------|-------|
| Sample rate | 16,000 / 22,050 / 32,000 / 44,100 / 48,000 / 88,200 / 96,000 / 176,400 / 192,000 / 384,000 Hz |
| Format | `i16` PCM |
| Minimum duration | ~2 seconds at the input sample rate |

Pass the input sample rate to `WakeWordModel::new()`. Non-16 kHz audio is resampled internally. Audio shorter than the minimum duration will return a score of `0.0` for all classifiers.

## Pre-trained models

The `onnx/` directory contains pre-trained models:

| File | Purpose |
|------|---------|
| `melspectrogram.onnx` | Mel spectrogram extraction (embedded at compile time) |
| `embedding_model.onnx` | Speech embedding generation (embedded at compile time) |
| `hey_livekit.onnx` | "Hey LiveKit" wake word classifier (loaded at runtime) |

## Training custom wake words

To train your own wake word classifiers, see the [livekit-wakeword](https://github.com/livekit/livekit-wakeword) Python toolkit. The exported `.onnx` classifier models can be loaded directly by this crate.
