---
"livekit-ffi": patch
---

Expose the audio stream and the audio resampler over uniffi alongside the existing protobuf requests. `AudioStream` is now a uniffi object with an `async next()` returning an `AudioFrameBuffer` that carries its samples, and the protobuf event path pumps the same method. `AudioResampler` is a uniffi object taking and returning that same buffer. Participant-sourced streams stay on the protobuf path.
