---
webrtc-sys: patch
libwebrtc: patch
---

`RtpTransceiver::current_direction` returns `None` before the transceiver is negotiated instead of aborting. The bridge called `value()` on the empty optional, which aborts on Windows, where the bridge is built without STL exceptions.
