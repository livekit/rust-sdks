---
livekit: patch
webrtc-sys: patch
---

Make AdmProxy worker-thread-affine: all platform ADM access now happens on the WebRTC worker thread, matching the ADM threading contract.

- The platform ADM is now created lazily on the first PlatformAudio acquire on all platforms, so apps that never use platform audio never construct it.
- Fixes Android platform recording delivering no audio: the audio transport was never registered on the lazily created ADM.
- Fixes a shutdown race by keeping the runtime threads alive as long as Rust can reach the audio device controller.
- Adds a `platform_audio` example exercising the PlatformAudio API and the worker-thread marshaling.
