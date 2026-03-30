---
soxr-sys: patch
yuv-sys: patch
livekit-api: patch
livekit-protocol: patch
webrtc-sys-build: patch
livekit: patch
libwebrtc: minor
webrtc-sys: patch
livekit-ffi: patch
imgproc: patch
livekit-wakeword: patch
---

# Add mechanism to allow creation of local track with platform ADM

#958 by @juberti-oai

Currently the PeerConnection is always created using a virtual audio device. However there are cases where it would be useful to use these Rust bindings with a real platform ADM.

This PR adds a new `with_platform_adm` constructor to opt into creating the PeerConnection with the platform default ADM, and a `create_audio_source` method to get a factory-backed audio source that can be used to create a local track.
