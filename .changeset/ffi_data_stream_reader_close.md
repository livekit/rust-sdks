---
livekit-data-stream: major
livekit-ffi: minor
livekit: major
livekit-capture: patch
livekit-rpc: patch
---

Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin), which ends a pending read with `DataStreamError::AlreadyClosed`, drops the stream from the incoming manager at once and reports it closed exactly once, without affecting other streams. Releasing a reader without `close()` ends its stream the same way. The core adds `StreamCloser`, returned by `closer()` on the concrete readers, to end a stream from outside its reader.

`backend::incoming::InputEvent` and `backend::incoming::OutputEvent` in `livekit-data-stream` (also reachable as `livekit::data_stream`) are now `#[non_exhaustive]`, so later versions can add events. A `match` on either enum outside the crate must now have a wildcard arm.
