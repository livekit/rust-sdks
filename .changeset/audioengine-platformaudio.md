---
livekit: minor
webrtc-sys: minor
---

Use the Apple AudioEngine ADM for PlatformAudio on iOS and macOS.

- The platform ADM on Apple platforms is now the AVAudioEngine based device with runtime switchable voice processing and device change handling.
- `prefer_hardware_processing` now defaults to `true` on macOS as well as iOS, so PlatformAudio uses Apple voice processing by default on both. Pass `prefer_hardware_processing: false` to keep WebRTC software processing.
- The ADM proxy forwards the platform voice processing interface (topology, path toggle, state) so WebRTC's audio processing resolution works through it when track audio options are applied.
- Muting a published device track now mutes the microphone in hardware instead of stopping and restarting recording. The ADM proxy delegates `IsStopOnMuteModeEnabled()` to the active recording ADM, so the voice engine drives `SetMicrophoneMute` on the AudioEngine ADM. Other platforms keep the default stop-on-mute behavior.
- New `PlatformAudio::set_mute_mode` / `mute_mode` expose the AudioEngine mute mechanism: `MuteMode::VoiceProcessing` (default, VPIO mute), `MuteMode::RestartEngine` (turns off the mic privacy indicator while muted), and `MuteMode::InputMixer`. Unsupported platforms return the new `AudioError::Unsupported`.
- The convenience toggles `set_echo_cancellation` / `set_auto_gain_control` / `set_noise_suppression` now delegate to `configure_audio_processing`, so `active_aec_type` and siblings always report the applied state.
