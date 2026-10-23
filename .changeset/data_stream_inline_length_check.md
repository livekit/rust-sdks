---
livekit-data-stream: patch
livekit-ffi: patch
livekit: patch
livekit-capture: patch
livekit-rpc: patch
---

An incoming single-packet (inline) data stream whose payload is shorter or longer than the length in its header now fails with `StreamError::Incomplete` or `StreamError::LengthExceeded`, as a chunked stream already does. Before, it ended successfully.
