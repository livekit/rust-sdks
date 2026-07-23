## 0.1.6 (2026-07-23)

### Fixes

#### Route LiveKit signalling through a pluggable transport (new `livekit-net` crate).

The signalling WebSocket and the two pre-connect HTTP GETs (validate, region discovery) now go through pluggable transport traits (`WsClient` for the WebSocket, `HttpClient` for request/response) resolved from a process-global registry with independent slots — a consumer can bring only HTTP, or only WebSocket. The new `livekit-net` crate owns the WebSocket/HTTP/TLS stack behind those traits and ships native (tokio / async-std) backends. Native builds are unchanged in behavior.

**Breaking (`livekit-api`, and `livekit` via `EngineError::Signal`):**

- `SignalError::WsError` is removed — `tungstenite` is no longer part of the public API. A failed WebSocket handshake now surfaces its HTTP status as `SignalError::Client`/`Server`; transport connection and close failures surface as the new `SignalError::Connection(String)` / `SignalError::Closed` variants (previously all collapsed into `Timeout`).
- `SignalError` is now `#[non_exhaustive]`, and gains a `SignalError::TransportNotConfigured` variant — returned when no transport is registered (host/foreign builds must call `livekit_net::set_ws_client` / `set_http_client` before connecting). This is a permanent configuration error; callers must not retry.
- The signalling WebSocket/HTTP/TLS crates are no longer transitive dependencies of `livekit-api`; TLS features delegate to `livekit-net`. Existing `signal-client-tokio` / `-async` / `-dispatcher` and TLS feature names are unchanged.

## 0.1.5 (2026-07-14)

### Features

- Expose data tracks core functionality

## 0.1.4 (2026-07-09)

### Features

- Add a Dart bindings target. Bumps the crate's UniFFI dependency from 0.30 to 0.31 to match the bindgen.

## 0.1.3 (2026-06-24)

### Fixes

- harden reconnect behaviour - #1148 (@lukasIO)

## 0.1.2 (2026-06-23)

### Fixes

- Upgrade protocol to v1.48.0

## 0.1.1 (2026-05-29)

### Fixes

- bump protocol to v1.46.4 - #1121 (@lukasIO)
