---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
---

Fix silent subscription failures in single-pc mode when the SFU reuses an existing empty transceiver for a new remote track. Also make `RtpTransceiver::mid()` safe to call on transceivers that haven't been negotiated yet — libwebrtc is built with `-fno-exceptions`, so `std::optional::value()` aborted the process instead of throwing.
