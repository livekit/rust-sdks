# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.7 (2026-10-20)

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

#### `livekit-protocol` exposes the conversation schema of a thread's database, as protocol's Go package

does: `agent::THREAD_SCHEMA_VERSION` and `agent::THREAD_SCHEMA_V1` (the SQL); the crates that
depend on it follow its bump.

## 0.1.6 (2026-10-12)

### Fixes

#### `VideoGrants` gains the `agent` grant and `Claims` the `kind` claim (with

`AccessToken::with_kind`), which the Go, Python and JS SDKs already carry. An
agent worker's token is `VideoGrants { agent: true }` and a simulated job's
participant token is `kind: "agent"`; neither could be minted from Rust before.
`livekit-uniffi` exposes both: `TokenOptions.kind`, `Claims.kind`, and `agent`
on its `VideoGrants` record.

**Breaking:** the four grants the server infers when absent -- `can_publish`,
`can_subscribe`, `can_publish_data`, `can_update_own_metadata` -- are now
`Option<bool>`, as in the Go and JS SDKs. `None` leaves the decision to the
server, and the new getters (`can_publish()`, `can_subscribe()`,
`can_publish_data()`, `can_update_own_metadata()`) read a token the way the
server does, `can_publish_data` falling back to `can_publish` included. Code
that set these fields writes `Some(..)`; code that read them uses the getters.
The same fields are optional on the `livekit-uniffi` record, and
`livekit-api` re-exports the crate as `livekit_api::access_token`, so both
carry the change.

Nothing at its default is written into the token any more: unset claims and
grants are omitted, as the server's own `omitempty` grants are. Verification
of existing tokens is unchanged.

## 0.1.5 (2026-10-07)

### Fixes

- Never wait on the peer when closing a signal stream: a half-open socket (no FIN/RST) left the reader parked in `recv()` forever, so the resume never dialled and `Room::close` hung. `SignalStream::close` now aborts the parked reader and bounds the writer's Close flush.

## 0.1.4 (2026-09-24)

### Fixes

- refactor(signaling): explicit signal lifecycle state machine - #1402 (@lukasIO)
- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.1.3 (2026-09-22)

### Fixes

#### Report the reconnect reason to the server when resuming.

Resumes previously sent no reason, so server-side telemetry could not attribute why Rust
clients reconnect — every resume looked like `RR_UNKNOWN`. The engine now records what caused
the episode (signal disconnected, publisher failed, subscriber failed) and reports it on each
resume attempt. The v0 signalling path was also missing the `reconnect_reason` query parameter
entirely, so it would not have been reported even if a reason had been supplied.

#### `VideoGrants` gains the `agent` grant and `Claims` the `kind` claim (with

`AccessToken::with_kind`), which the Go, Python and JS SDKs already carry. An
agent worker's token is `VideoGrants { agent: true }` and a simulated job's
participant token is `kind: "agent"`; neither could be minted from Rust before.
`livekit-uniffi` exposes both: `TokenOptions.kind`, `Claims.kind`, and `agent`
on its `VideoGrants` record.

**Breaking:** the four grants the server infers when absent -- `can_publish`,
`can_subscribe`, `can_publish_data`, `can_update_own_metadata` -- are now
`Option<bool>`, as in the Go and JS SDKs. `None` leaves the decision to the
server, and the new getters (`can_publish()`, `can_subscribe()`,
`can_publish_data()`, `can_update_own_metadata()`) read a token the way the
server does, `can_publish_data` falling back to `can_publish` included. Code
that set these fields writes `Some(..)`; code that read them uses the getters.
The same fields are optional on the `livekit-uniffi` record, and
`livekit-api` re-exports the crate as `livekit_api::access_token`, so both
carry the change.

Nothing at its default is written into the token any more: unset claims and
grants are omitted, as the server's own `omitempty` grants are. Verification
of existing tokens is unchanged.

## 0.1.2 (2026-09-09)

### Fixes

- log warning if signal messages get dropped during reconnect - #1391 (@lukasIO)

## 0.1.1 (2026-09-08)

### Features

- Removes livekit-runtime and converts this package to be tokio only again - #1375 (@1egoman)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)

#### Moves the signalling client into a new `livekit-signaling` crate. livekit-api

re-exports it under the historical `livekit_api::signal_client` path, now marked
deprecated: it is internal SDK API, and dependents should use livekit-signaling
directly. livekit-api no longer depends on livekit-net.

Also drops two dependencies that were declared but never used: `scopeguard` and
`bytes`.

## 0.1.0

The initial release. This was broken out of [livekit-api](../livekit-api) into its own crate.
