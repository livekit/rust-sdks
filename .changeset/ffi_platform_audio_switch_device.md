---
livekit-ffi: patch
---

`SetRecordingDeviceRequest` and `SetPlayoutDeviceRequest` now switch the device in place on desktop while audio is active, instead of failing with "device not found".
