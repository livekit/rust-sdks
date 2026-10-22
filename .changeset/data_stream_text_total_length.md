---
livekit-data-stream: minor
livekit-ffi: minor
livekit: minor
livekit-capture: patch
livekit-rpc: minor
---

Add an optional `total_length` to `StreamTextOptions` in the core and in the `core-modules` record, so an incremental text stream can declare its length in the header. The receiver then reports progress against it and fails the stream if the length does not match. The new public field breaks struct-literal construction of `StreamTextOptions`, which `livekit` re-exports and the `livekit-rpc` transport trait takes.
