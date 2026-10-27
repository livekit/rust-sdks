---
livekit-data-stream: major
livekit: major
livekit-rpc: major
livekit-ffi: patch
livekit-capture: patch
---

Split the outgoing data stream options by call and make every options struct `#[non_exhaustive]`. One-shot sends take the new `SendTextOptions` (`send_text`) and `SendByteOptions` (`send_bytes`, `send_file`), which carry `compress` and a new `on_progress` callback. Incremental streams keep `StreamTextOptions` (`stream_text`) and `StreamByteOptions` (`stream_bytes`), which carry `total_length`; `StreamTextOptions` gains that field and both lose `compress`, which they ignored. `Manager::send_file_with_progress` and `Manager::stream_text_with_total_length` are gone: set `on_progress` and `total_length` on the options instead. Build every options value with `new_with_topic` and the `with_*` setters; struct literals and `..` updates no longer compile outside the crate. `RpcTransport::send_text` now takes `SendTextOptions`. The `core-modules` FFI records and the generated Swift and Kotlin surface are unchanged.
