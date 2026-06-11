---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-ffi: patch
---

Fix AV1 subscriber decode when packet trailers are enabled, including when E2EE
is active. E2EE encrypts the entire AV1 payload, which the OBU-parsing AV1 RTP
packetizer cannot transport intact; encrypted AV1 payloads are now wrapped in a
synthetic AV1 temporal unit after encryption (and unwrapped before decryption)
so they survive packetization and SFU keyframe detection. Wrapped keyframes
carry the stream's real sequence header (captured from the plaintext before
encryption, mirroring how H264 E2EE leaves SPS/PPS readable) so SFUs parsing it
observe the true stream parameters.

Also fix encrypted video tracks losing their encryption/decryption transform
when a packet trailer handler was created lazily (e.g. by consuming publish or
subscribe timing events on a track without negotiated trailer features): in
E2EE rooms the handler is now always created up front and chained with the
frame cryptor.
