---
webrtc-sys: patch
---

fix: repair two libwebrtc patches that had silently stopped applying

- `external_audio_source.patch`: the `api/media_stream_interface.h` hunk assumed
  `options()` was the last member of `AudioSourceInterface`. webrtc-sdk/webrtc#247
  added `SetOptions()` after it in June, so the patch has not applied since, and
  `is_external_source()` has been missing from shipped builds.
- `jni_prefix.patch`: the `generated_peerconnection_jni` hunk anchored on
  `TurnCustomizer.java` being the last entry of that source list. Six entries have
  been appended since, so the Android JNI package prefix was not being applied.

Neither failure was visible because the build scripts apply patches with
`git apply ... || true`, so a patch that stops applying is skipped and the build
still succeeds without the feature.
