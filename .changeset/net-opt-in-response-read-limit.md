---
livekit-net: major
livekit-ffi: major
livekit-signaling: major
livekit-token-source: major
livekit-telemetry: major
livekit: major
livekit-capture: major
livekit-api: patch
---

Add an opt-in decoded HTTP response limit with native read-time enforcement while preserving ordinary request behavior. The new `TransportError::ResponseTooLarge { headers }` variant breaks Rust code that matches `TransportError` exhaustively and Swift `switch` or Kotlin `when` statements over the generated `TransportError`, so add a case for it. The livekit-net 0.2 types reach other public APIs, so these crates take the same break: `livekit-signaling` (`From<TransportError>` for `SignalError`), `livekit-token-source` (`TokenSourceError::Transport`), `livekit-telemetry` (`From<HttpResponse>` for `ExportResponse` with `net`), `livekit` (`From<RoomSdkOptions>` for the signaling `SignalSdkOptions`) and `livekit-capture` (returns `livekit` types such as `TrackPublishOptions`). Signaling maps the new variant to `SignalError::Connection`.
