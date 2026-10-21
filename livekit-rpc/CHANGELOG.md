# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.5 (2026-10-20)

### Fixes

- Outgoing data stream chunk and trailer packets now carry the stream's destination identities. Before, only the header was targeted, so the content of a stream sent to specific participants was delivered to everyone in the room.

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

#### Stop failing an RPC call whose reply arrives just after its recipient leaves the room.

Purging a call's pending ack and response the instant its recipient disconnects assumed that
"participant left" implies "no reply is coming". It does not: the disconnect is announced on the
signalling connection while the ack and response travel on the data channel, and nothing orders
the two. A recipient that leaves *because of* the RPC it just answered — `lk.sip.EndCall`, where
the SIP participant drops out of the room as soon as it has ended the call — produces that order
routinely, so the reply was discarded ("Response received for unexpected RPC request") and the
caller was handed `RecipientDisconnected` for a call that in fact succeeded.

A disconnect now shortens the wait instead of ending it: the pending entries stay registered for
up to 2 more seconds, so a reply already in flight still resolves the caller, and a recipient that
really did leave without replying still fails fast rather than running out the 15s response
timeout.

## 0.1.4 (2026-10-12)

### Fixes

- In-flight incoming data streams now error with `AbnormalEnd` when the room closes. Before, the stream readers ended cleanly, so `read_all()` returned truncated content as if it were complete.

#### `livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,

`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.

## 0.1.3 (2026-09-24)

### Fixes

- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.1.2 (2026-09-22)

### Fixes

- Fix reliable data channel replay: keep the full retry buffer across resumes, drop duplicate reliable packets, and ignore replayed chunks on uncompressed streams instead of failing with `MissedChunk`.

## 0.1.1 (2026-09-09)

### Fixes

#### Moves the RPC implementation into a new `livekit-rpc` crate, alongside the existing

`livekit-data-stream` and `livekit-datatrack` crates.

**Breaking:** the `livekit::rpc` module is gone. The RPC types it held are unchanged and
still re-exported from `livekit::participant` and the prelude, so most code needs no edit;
code that spelled the module out (`use livekit::rpc::RpcError;`) should import from
`livekit::participant` or the prelude instead. `RpcClientManager`, `RpcServerManager` and
`HandleRequestOptions` remain reachable under `livekit::participant` but are now
`#[doc(hidden)]`: they are internal SDK API and were never usable without the (private)
transport trait.

Within the `livekit` crate itself, RPC types are now imported from `livekit-rpc` directly
rather than through those re-exports.

The new crate does not depend on `libwebrtc`, so its unit tests run without building WebRTC.
The transport seam that made this possible was already in place; the only change to it is
that `RpcTransport::publish_data` now returns a message-only `RpcTransportError` instead of
`livekit::RoomError`, mirroring `livekit_data_stream::api::SendError`.

Also fixes four latent bugs found while moving the code:

- An RPC call to a participant who disconnects mid-call now fails promptly with
  `RecipientDisconnected`. Pending calls were never purged on disconnect, so the caller
  waited out its full response timeout (15s by default) and got `ResponseTimeout` instead.
- A server reporting a version that is not valid semver no longer panics the calling task.
  An unparseable version is no longer treated as evidence that the server is too old.
- A v1 `RpcResponse` carrying a compressed payload, or no value at all, now fails with an
  `ApplicationError` instead of resolving the caller with an empty successful response.
- Removed an unguarded `unwrap` when building a v1 response packet, by giving the function
  a signature that cannot represent the invalid state.

Also drops the `semver` dependency from `livekit`, which was only used by the RPC client.

## 0.1.0

The initial release. This was broken out of [livekit](../livekit) into its own crate.
