---
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-ffi: patch
livekit-signaling: minor
livekit-uniffi: patch
---

Report the reconnect reason to the server when resuming.

Resumes previously sent no reason, so server-side telemetry could not attribute why Rust
clients reconnect — every resume looked like `RR_UNKNOWN`. The engine now records what caused
the episode (signal disconnected, publisher failed, subscriber failed) and reports it on each
resume attempt. The v0 signalling path was also missing the `reconnect_reason` query parameter
entirely, so it would not have been reported even if a reason had been supplied.

`livekit-signaling` carries a breaking change: `SignalClient::restart` now takes the
`ReconnectReason` to report. It is public API, so the crate takes a minor bump rather than a
patch — under Cargo's semver rules a `0.1.x` patch would be resolved as compatible and break
callers on upgrade.
