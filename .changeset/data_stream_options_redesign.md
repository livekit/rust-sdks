---
livekit-data-stream: major
livekit: major
livekit-rpc: major
livekit-ffi: patch
livekit-capture: patch
---

Split outgoing data stream options into `SendTextOptions`, shared `SendByteOptions`, `StreamTextOptions` and `StreamByteOptions`, all `#[non_exhaustive]`. Remove `send_file_with_progress` and `stream_text_with_total_length` in favor of `on_progress` and `total_length` on the options.
