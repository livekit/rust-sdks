---
livekit-uniffi: minor
---

# Add UniFFI interface for livekit-wakeword

## Summary
- Expose `WakeWordDetector` as a UniFFI Object wrapping `WakeWordModel` with `Mutex` for interior mutability
- Export `new`, `load_model`, and `predict` methods across FFI
- Use `#[uniffi::remote(Error)]` with `#[uniffi(flat_error)]` for `WakeWordError`, matching the existing `AccessTokenError` pattern
