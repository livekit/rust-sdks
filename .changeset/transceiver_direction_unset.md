---
libwebrtc: patch
webrtc-sys: patch
---
Reading a transceiver's current or fired direction before it is negotiated returns `None` instead of aborting the process.
