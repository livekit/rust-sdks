---
livekit-data-stream: major
livekit-ffi: minor
livekit: major
livekit-capture: patch
livekit-rpc: patch
---

Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin) and core `StreamCloser` handles returned by `closer()` to end pending reads with `DataStreamError::AlreadyClosed` and remove streams exactly once; dropping a reader has the same effect. Mark the core incoming `InputEvent` and `OutputEvent` enums as `#[non_exhaustive]`.
