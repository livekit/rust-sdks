---
webrtc-sys: patch
libwebrtc: patch
---

Fix a crash in `RtpSender::track` and `RtpReceiver::track` when there is no track, such as on a transceiver added by media type: they now return `None`.
