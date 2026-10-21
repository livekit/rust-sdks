## 0.1.4 (2026-10-20)

### Fixes

#### Merge `livekit-uniffi` into `livekit-ffi`.

There is now one crate, one version and one release tag. The `livekit-uniffi`
crate, its changelog and its `livekit-uniffi/v*` tags are gone; its history is
preserved in git.

The crate carries two mutually exclusive surfaces, selected by feature:

- `room-apis` (default) is the existing protobuf / C-ABI surface, with
  `livekit` and libwebrtc. Everything that built `livekit-ffi` before builds it
  unchanged.
- `core-modules` is the former `livekit-uniffi` surface — access tokens, log
  forwarding, data tracks, data streams v2 — and depends on none of `livekit`,
  `libwebrtc`, `soxr-sys` or `imgproc`. What previously built `livekit-uniffi`
  now builds `-p livekit-ffi --no-default-features --features core-modules`.

`[package.metadata.platform-features]` in `livekit-ffi/Cargo.toml` records
which surface each downstream package build gets, and CI resolves its build
flags from it.

The merge itself requires no downstream SDK changes: the UniFFI namespace is
pinned per surface and every published package name is preserved. The one
difference is the native library the crate builds, `liblivekit_uniffi` →
`liblivekit_ffi`, which is loaded by name from the generated bindings and the
Dart build hook and needs no consumer change. The package renames are a
separate change, released alongside this one.

The UniFFI node package (`@livekit/uniffi`) is not built for now and will be
reintroduced separately; node bindings ship from `livekit-ffi-node-bindings`.

## 0.1.3 (2026-09-08)

### Features

- Removes livekit-runtime and converts this package to be tokio only again - #1375 (@1egoman)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)
- Add `self_test_http_get` / `self_test_ws_echo` / `has_http_client` / `has_ws_client` UniFFI exports so foreign hosts can exercise the transport seam end-to-end.

## 0.1.2 (2026-07-29)

### Fixes

- Caching of tokio backend reqwest http client - #1285 (@MaxHeimbrock)

## 0.1.1 (2026-07-27)

### Features

#### Route LiveKit signalling through a pluggable transport (new `livekit-net` crate).

The signalling WebSocket and the two pre-connect HTTP GETs (validate, region discovery) now go through pluggable transport traits (`WsClient` for the WebSocket, `HttpClient` for request/response) resolved from a process-global registry with independent slots — a consumer can bring only HTTP, or only WebSocket. The new `livekit-net` crate owns the WebSocket/HTTP/TLS stack behind those traits and ships native (tokio / async-std) backends. Native builds are unchanged in behavior.

**Breaking (`livekit-api`, and `livekit` via `EngineError::Signal`):**

- `SignalError::WsError` is removed — `tungstenite` is no longer part of the public API. A failed WebSocket handshake now surfaces its HTTP status as `SignalError::Client`/`Server`; transport connection and close failures surface as the new `SignalError::Connection(String)` / `SignalError::Closed` variants (previously all collapsed into `Timeout`).
- `SignalError` is now `#[non_exhaustive]`, and gains a `SignalError::TransportNotConfigured` variant — returned when no transport is registered (host/foreign builds must call `livekit_net::set_ws_client` / `set_http_client` before connecting). This is a permanent configuration error; callers must not retry.
- The signalling WebSocket/HTTP/TLS crates are no longer transitive dependencies of `livekit-api`; TLS features delegate to `livekit-net`. Existing `signal-client-tokio` / `-async` / `-dispatcher` and TLS feature names are unchanged.
