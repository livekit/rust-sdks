---
livekit-data-stream: patch
livekit-ffi: patch
livekit: patch
livekit-capture: patch
livekit-rpc: patch
---

A data stream that its sender closes early with a reason now fails with that reason (`StreamError::AbnormalEnd`), as in JS, even when its header declared a total length; before, the reader got `StreamError::Incomplete`.
