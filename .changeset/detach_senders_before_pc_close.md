---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Release local tracks when the server ends the room by detaching publisher senders before the publisher PeerConnection closes, and make `RtpSender::track()` return `None` for a detached sender instead of dereferencing null.
