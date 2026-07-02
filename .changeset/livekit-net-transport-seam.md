---
livekit-api: major
livekit: major
livekit-uniffi: minor
---

Route LiveKit signalling through a pluggable, host-providable transport (new `livekit-net` crate).

The signalling WebSocket and the two pre-connect HTTP GETs (validate, region discovery) now go through a transport trait resolved from a process-global registry, so mobile/UniFFI builds can drop the Rust TLS/WebSocket/HTTP stack and use the platform's network stack. Native desktop builds are unchanged in behavior.

**Breaking (`livekit-api`, and `livekit` via `EngineError::Signal`):**

- `SignalError::WsError` is removed — `tungstenite` is no longer part of the public API. A failed WebSocket handshake now surfaces its HTTP status as `SignalError::Client`/`Server`; transport connection and close failures surface as the new `SignalError::Connection(String)` / `SignalError::Closed` variants (previously all collapsed into `Timeout`).
- `SignalError` is now `#[non_exhaustive]`.
- The signalling WebSocket/HTTP/TLS crates are no longer transitive dependencies of `livekit-api`; TLS features delegate to `livekit-net`. Existing `signal-client-tokio` / `-async` / `-dispatcher` and TLS feature names are unchanged.

`livekit` adds a `foreign` feature that pairs the backend-agnostic signalling client with a host-provided transport.

`livekit-uniffi` gains `setPlatformTransport` so a Dart/Swift/Kotlin host can inject its own transport across the UniFFI boundary, and drops the services (reqwest) stack from the cdylib.
