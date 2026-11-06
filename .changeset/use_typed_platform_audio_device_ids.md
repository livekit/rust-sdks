---
livekit: minor
livekit-ffi: patch
---

refactor: route platform audio device selection through typed device IDs in Rust and FFI.

fix: narrow hidden platform audio index/count helpers to internal APIs and use `usize` for public device indices.

fix: return `DeviceNotFound` when selecting an unavailable platform audio device ID.
