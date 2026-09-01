## 0.1.9 (2026-08-25)

### Fixes

- differentiate signal connection errors correctly from timeouts - #1234 (@lukasIO)
- Expose `RemoteDataTrack.set_pipeline_options` and `RemoteDataTrackPipelineOptions` (`max_partial_frames`) over UniFFI, matching the JS and Rust SDKs
- Register the `Bytes` UniFFI custom type once in `livekit-common` and borrow it from each component with `uniffi::use_remote_type!`, so the converter is emitted once and the post-generation Swift workaround is no longer needed

#### Moves access-token generation and verification into a new `livekit-token` crate.

`livekit_api::access_token::*` continues to resolve to the same types via a
re-export, so no consumer changes are needed.

Also fixes the `services-tokio` and `services-async` features, which used the
access-token types without declaring the `access-token` feature. Building with
`--no-default-features --features services-tokio` previously failed to compile.

## 0.1.8 (2026-08-03)

### Fixes

- Add a unified `EgressClient::start_egress` that calls the v2 `Egress.StartEgress` RPC with a `StartEgressRequest`, alongside the existing per-type helpers.
- Fix the Android AAR build. Two cargo-make bugs kept `cargo make --profile release android-package` from ever producing a release artifact: the per-arch tasks' `env = { TARGET = ... }` replaced (rather than merged) the parent env map, dropping the `--release` flag, and the `TARGET` they set leaked into the Kotlin bindgen's host build, which then cross-compiled with the host linker. Also raise the Swift and Android size budgets to match the binaries as they stand since the data-track UniFFI surface landed, and check out the released tag rather than the dispatch ref when building the wrapper packages.

## 0.1.7 (2026-07-29)

### Fixes

- Caching of tokio backend reqwest http client - #1285 (@MaxHeimbrock)
- Add data streams v2 - #1192 (@1egoman)

## 0.1.6 (2026-07-27)

### Fixes

- Data tracks schema metadata support.
- Gate the uniffi `cli` feature (clap + bindgen backends) behind an opt-in feature so it is no longer compiled into shipped library builds, shrinking the static archive by ~43 MiB. The dynamic library is byte-unchanged. - #1275 (@jhugman)

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
