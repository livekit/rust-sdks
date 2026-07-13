---
livekit: minor
webrtc-sys: minor
---

Mute PlatformAudio microphone tracks in hardware on Apple platforms.

- The ADM proxy now delegates `IsStopOnMuteModeEnabled()` to the active recording ADM, so muting a published device track drives `SetMicrophoneMute` on the Apple AudioEngine ADM instead of stopping and restarting recording. Other platforms keep the default stop-on-mute behavior.
- New `PlatformAudio::set_mute_mode` / `mute_mode` expose the AudioEngine mute mechanism: `MuteMode::VoiceProcessing` (default, VPIO mute), `MuteMode::RestartEngine` (turns off the mic privacy indicator while muted), and `MuteMode::InputMixer`. Unsupported platforms return the new `AudioError::Unsupported`.
- `SetMicrophoneMute` / `MicrophoneMute` on the ADM proxy are now gated on active platform recording, so mute state cannot leak into an idle ADM.
