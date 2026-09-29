---
livekit-ffi: patch
---

Expose the video stream handle over uniffi alongside the existing protobuf requests. `VideoStream` is now a uniffi object with an `async next()` returning a `VideoFrame` that carries its pixels, and the protobuf event path pumps the same method. Participant-sourced streams stay on the protobuf path.
