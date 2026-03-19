---
livekit-ffi: patch
webrtc-sys-build: patch
yuv-sys: patch
imgproc: patch
livekit-protocol: patch
webrtc-sys: patch
livekit: patch
libwebrtc: patch
livekit-wakeword: patch
soxr-sys: patch
livekit-api: patch
---

# fix PC timeout when connecting with can_subscribe=false

#955 by @s-hamdananwar

When a participant connects with `canSubscribe=false` in their token, the server sends `subscriber_primary=false` in the JoinResponse and does not send a subscriber offer.  This results in `wait_pc_connection` timing out as it is expecting a subscriber PC even when the publisher PC is primary. This PR will skip waiting for subscriber PC when `subscriber_primary=false`. 
