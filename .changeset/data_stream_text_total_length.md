---
livekit-data-stream: minor
livekit-ffi: minor
livekit: minor
livekit-capture: patch
livekit-rpc: patch
---

Let an incremental text stream declare its total length in the header: the core adds `StreamTextOptions::total_length`, and the `core-modules` `StreamTextOptions` record gets an optional `total_length` that `stream_text` passes to it. The receiver then reports progress against it and fails the stream if the length does not match.
