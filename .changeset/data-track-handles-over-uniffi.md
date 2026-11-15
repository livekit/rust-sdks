---
livekit-ffi: patch
---

Expose the local and remote data track handles over uniffi alongside the existing protobuf requests. `LocalDataTrack` and `RemoteDataTrack` are now uniffi objects forwarding to the underlying `livekit-datatrack` types, and the protobuf requests keep working against the same handle.
