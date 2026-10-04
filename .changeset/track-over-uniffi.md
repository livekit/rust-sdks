---
"livekit-ffi": patch
---

Expose the track handle over uniffi alongside the existing protobuf requests. `Track` is now a uniffi object with constructors over a video or audio source, accessors for its name, sid, kind, stream state and mute state, and `set_muted`/`set_enabled`. `VideoStream` and `AudioStream` take that object in place of a track handle id. Track stats stay on the protobuf path.
