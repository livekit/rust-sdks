---
livekit-ffi: patch
---

Return an error from `SoxResampler` creation for zero, negative or non-finite sample rates and for zero channels, instead of creating a resampler that aborts the process on the first push.
