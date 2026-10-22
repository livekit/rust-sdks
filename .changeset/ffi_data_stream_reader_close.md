---
livekit-ffi: minor
---

Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin). It ends a pending read with `DataStreamError::AlreadyClosed` and releases the data the stream buffered, without affecting other streams.
