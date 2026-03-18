---
soxr-sys: patch
webrtc-sys: patch
livekit-ffi: minor
livekit-protocol: patch
livekit: minor
livekit-wakeword: patch
livekit-api: patch
imgproc: patch
libwebrtc: minor
webrtc-sys-build: patch
yuv-sys: patch
---

# E2EE: allow setting key_ring_size and key_derivation_algorithm, update webrtc to m144

#921 by @onestacked

This PR uses [this webrtc-sdk PR](https://github.com/webrtc-sdk/webrtc/pull/224) to configure the KDF.

I've tested this with https://codeberg.org/esoteric_programmer/matrix-jukebox and it is compatible with Element Call.

Since this PR needs to use a new webrtc build it also updates webtc to m144. See [this PR](https://github.com/webrtc-sdk/webrtc/pull/217)

Fixed: https://github.com/livekit/rust-sdks/issues/796