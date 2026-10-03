---
webrtc-sys: patch
---

Keep `NativeAudioSource` send streams out of WebRTC's `AudioState`: `AudioSendStream::Stop()` sets `sending_ = false` again, and `StoreEncoderProperties()` no longer adds external sources. A stream added there was never removed, so with `PlatformAudio` recording, the capture thread called into it from `AudioTransportImpl::SendProcessedData` after it was destroyed.
