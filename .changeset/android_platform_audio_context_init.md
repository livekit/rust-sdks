---
livekit-ffi: minor
livekit: patch
libwebrtc: patch
webrtc-sys: patch
---

feat: add Android application context initialization for PlatformAudio support.

Android requires `ContextUtils.initialize(applicationContext)` before WebRTC audio components can be created. This change:

- Adds `livekit_ffi_initialize_android_context()` C FFI function for Unity and other FFI consumers
- Uses `CreateAndroidAudioDeviceModule()` instead of generic `CreateAudioDeviceModule()` on Android
- Handles empty device GUIDs on Android (falls back to index 0)
- Documents Android-specific limitations: single default device, no app-level device selection

Platform notes:
- Android device enumeration returns only one "default" device with empty name/GUID
- Audio routing (speaker/earpiece/Bluetooth) is controlled by Android's AudioManager, not WebRTC
