---
livekit-ffi: patch
---

# Export SoxResampler through UniFFI

`SoxResampler` is now a UniFFI object, so foreign hosts can construct one and
drive `push` / `flush` directly, receiving the resampled samples by value.

The FFI request surface is unchanged: `NewSoxResampler`, `PushSoxResampler` and
`FlushSoxResampler` still hand back a pointer into the resampler's own buffer,
readable until the next call on that resampler. Both paths are covered by new
tests that drive them with the same input and compare the results.
