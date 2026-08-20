---
libwebrtc: minor
livekit: minor
webrtc-sys: patch
livekit-ffi: patch
---

feat: enable WARP (SPED + SNAP) by default, gated by the server

WARP is now always enabled on the client and negotiated with the SFU: SPED
(DTLS-in-STUN) via the `WebRTC-IceHandshakeDtls` field trial, and SNAP
(SCTP-INIT-in-SDP) via the `RtcConfiguration.enable_sctp_snap` field. When the
server does not enable WARP it is not advertised and the connection falls back to
plain DTLS/SCTP, so there is no client-side toggle.

BREAKING CHANGE: `libwebrtc::RtcConfiguration` is now `#[non_exhaustive]` and has a
new `enable_sctp_snap` field. Construct it from `RtcConfiguration::default()` and set
the fields you need instead of a struct literal.
