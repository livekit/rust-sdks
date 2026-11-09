---
livekit-net: major
livekit-ffi: major
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-signaling: major
livekit-token-source: major
---

Add an opt-in decoded HTTP response limit with native read-time enforcement while preserving ordinary request behavior. The new `TransportError::ResponseTooLarge { headers }` variant breaks Rust code that matches `TransportError` exhaustively and Swift `switch` or Kotlin `when` statements over the generated `TransportError`, so add a case for it. `livekit-signaling` (its public `From<TransportError>` for `SignalError`) and `livekit-token-source` (`TokenSourceError::Transport`) expose the type, so they take the same break. Signaling maps the new variant to `SignalError::Connection`.
