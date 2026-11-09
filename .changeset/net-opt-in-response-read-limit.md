---
livekit-net: major
livekit-ffi: major
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-signaling: patch
livekit-token-source: patch
---

Add an opt-in decoded HTTP response limit with native read-time enforcement while preserving ordinary request behavior. The new `TransportError::ResponseTooLarge { headers }` variant breaks Rust code that matches `TransportError` exhaustively and Swift `switch` or Kotlin `when` statements over the generated `TransportError`, so add a case for it.
