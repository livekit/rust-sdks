---
webrtc-sys: patch
---

Copy peer connection session descriptions on the WebRTC signaling thread to prevent crashes when a description is replaced concurrently.
