---
livekit: patch
livekit-signaling: patch
livekit-ffi: patch
---

Remove the `use_legacy_data_streams` migration flag. The FFI `RoomDataStreamOptions.use_legacy_client_implementation` field (now reserved), the hidden `RoomDataStreamOptions::with_legacy_client_implementation` builder and the hidden `SignalOptions::use_legacy_data_streams` field are gone; clients always advertise data streams v2 support.
