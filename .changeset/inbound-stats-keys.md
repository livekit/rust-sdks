---
libwebrtc: patch
livekit-ffi: patch
---

Fix inbound-rtp `total_freeze_duration` and `total_pause_duration` and outbound-rtp `scalibility_mode`, which always read 0 or empty because they were mapped to the wrong libwebrtc stats keys.
