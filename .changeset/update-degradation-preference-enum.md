---
livekit: minor
livekit-ffi: minor
libwebrtc: minor
webrtc-sys: minor
---

Add `MaintainFramerateAndResolution` to `DegradationPreference` enum to align with WebRTC M144.

- `MAINTAIN_FRAMERATE_AND_RESOLUTION` is now the recommended value (replaces deprecated `DISABLED`)
- `DISABLED` is deprecated but still supported for backwards compatibility
- Both values map to the same behavior: maintain framerate and resolution, dropping frames if needed
