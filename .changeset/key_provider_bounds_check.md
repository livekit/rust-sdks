---
"libwebrtc": patch
"livekit": patch
"webrtc-sys": patch
---

# Handle out-of-range key indices gracefully in KeyProvider

Bounds-check `key_index` at the native and wrapper boundaries in `KeyProvider`:
- Out-of-bounds `set_key` and `set_shared_key` return `false` instead of aborting the process.
- Out-of-bounds `get_key`, `ratchet_key`, `get_shared_key`, and `ratchet_shared_key` return `None` (or error in FFI) instead of aborting the process.
- `key_ring_size` in `KeyProviderOptions` is clamped to valid native bounds `1..=255` (exposed as `MAX_KEY_RING_SIZE`).

