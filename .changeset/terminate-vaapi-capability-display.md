---
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
webrtc-sys: patch
---

Terminate the temporary VAAPI display after encoder capability detection so repeated room creation does not retain VA driver resources and threads.
