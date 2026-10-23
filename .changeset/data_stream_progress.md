---
livekit-data-stream: minor
livekit-ffi: minor
livekit: minor
livekit-capture: patch
livekit-rpc: patch
---

Add data stream progress to `core-modules`: readers get `next_progress()`, which counts received bytes, and `send_text`, `send_bytes` and `send_file` take an optional `progress` delegate (`StreamProgressDelegate`) that defaults to none. The core adds `send_file_with_progress` and a `progress_stream()` accessor on the concrete readers that does not borrow the reader.
