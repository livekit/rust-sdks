---
livekit-data-stream: minor
livekit-ffi: minor
livekit: minor
livekit-capture: patch
livekit-rpc: patch
---

Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin), which ends a pending read with `DataStreamError::AlreadyClosed`, drops the stream from the incoming manager at once and reports it closed exactly once, without affecting other streams. Releasing a reader without `close()` ends its stream the same way. The core adds `StreamCloser`, returned by `closer()` on the concrete readers, to end a stream from outside its reader.
