---
livekit: minor
webrtc-sys: minor
---

Use the Apple AudioEngine ADM for PlatformAudio on iOS and macOS.

- The platform ADM on Apple platforms is now the AVAudioEngine based device with runtime switchable voice processing and device change handling.
- `prefer_hardware_processing` now defaults to `true` on macOS as well as iOS, so PlatformAudio uses Apple voice processing by default on both. Pass `prefer_hardware_processing: false` to keep WebRTC software processing.
- The ADM proxy forwards the platform voice processing interface (topology, path toggle, state) so WebRTC's audio processing resolution works through it when track audio options are applied.
