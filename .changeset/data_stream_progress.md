---
livekit-data-stream: minor
livekit-ffi: minor
livekit: patch
livekit-capture: patch
livekit-rpc: patch
---

Add data stream progress to `core-modules`: readers get `next_progress()`, and `send_text_with_progress`, `send_bytes_with_progress` and `send_file_with_progress` report to a `StreamProgressDelegate`. The core adds `send_file_with_progress`, and the stream from `StreamReader::progress()` no longer borrows the reader.
