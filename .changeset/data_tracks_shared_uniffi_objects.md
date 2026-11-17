---
livekit-ffi: patch
---

Expose `LocalDataTrack`, `RemoteDataTrack` and `DataTrackStream` over UniFFI in room-apis builds too, as the same objects core-modules exports. A track handle from the C ABI (`OwnedLocalDataTrack`, `OwnedRemoteDataTrack`) can be taken up with `from_ffi_handle_id` and handed back with `ffi_handle_id`, as with `SoxResampler`; the protobuf request surface is unchanged. A local track taken up over UniFFI is co-owned: dropping its C ABI handle no longer unpublishes it while the UniFFI side still holds it.
