---
livekit-ffi: patch
---

Make `sample_rate` and `num_channels` optional in `NewAudioSourceRequest`.

These fields are ignored for `AudioSourcePlatform` (ADM uses hardware native settings) and for `AudioSourceNative` fast path (queue_size_ms=0, frame values used directly). Defaults to 48000 Hz and 1 channel when not specified.
