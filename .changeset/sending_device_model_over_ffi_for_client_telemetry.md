---
livekit-ffi: minor
---

# Sending device model over ffi for client telemetry

#966 by @MaxHeimbrock

Send device_model in client telemetry if set from ffi. This adds a parameter `livekit_ffi_initialize`, all ffi clients must provide the device_model or null. 
