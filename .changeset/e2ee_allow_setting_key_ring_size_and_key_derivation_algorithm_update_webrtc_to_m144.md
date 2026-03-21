---
libwebrtc: minor
livekit-ffi: minor
livekit: minor
webrtc-sys: patch
---

# E2EE: allow setting key_ring_size and key_derivation_algorithm, update webrtc to m144

#921 by @onestacked

This PR uses [this webrtc-sdk PR](https://github.com/webrtc-sdk/webrtc/pull/224) to configure the KDF.

I've tested this with https://codeberg.org/esoteric_programmer/matrix-jukebox and it is compatible with Element Call.

Fixed: https://github.com/livekit/rust-sdks/issues/796