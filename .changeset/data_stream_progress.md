---
livekit-data-stream: patch
livekit-ffi: minor
livekit: patch
livekit-capture: patch
livekit-rpc: patch
---

Add data stream progress to `core-modules`: readers get `next_progress()`, which counts received bytes, and `send_text_with_progress`, `send_bytes_with_progress` and `send_file_with_progress` report to a `StreamProgressDelegate`. The core adds `send_file_with_progress` and a `progress_stream()` accessor on the concrete readers that does not borrow the reader.
