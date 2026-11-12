---
livekit-bitstream: patch
livekit-capture: patch
---

Moves the H.264/H.265 NAL unit helpers from `livekit-capture` into a new internal `livekit-bitstream` crate, with no public API or behavior change.
