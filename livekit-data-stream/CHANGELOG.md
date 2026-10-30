## 0.2.0 (2026-10-28)

### Breaking Changes

- `InputEvent` and `OutputEvent` are `#[non_exhaustive]`; exhaustive matches need a wildcard arm.
- Split outgoing data stream options into `SendTextOptions`, shared `SendByteOptions`, `StreamTextOptions` and `StreamByteOptions`, all `#[non_exhaustive]`. Remove `send_file_with_progress` and `stream_text_with_total_length` in favor of `on_progress` and `total_length` on the options.

### Features

- Add data stream progress to `core-modules`: readers get `next_progress()`, which counts received bytes, and `send_text`, `send_bytes` and `send_file` take an optional `progress` delegate (`StreamProgressDelegate`) inside their options record, defaulting to none and ignored by incremental streams. The core adds `on_progress` on `SendTextOptions` and `SendByteOptions` and a `progress_stream()` accessor on the concrete readers that does not borrow the reader. The generated Swift option records no longer conform to `Equatable` or `Hashable`.
- Let an incremental text stream declare its total length in the header: the core adds `StreamTextOptions::total_length`, and the `core-modules` `StreamTextOptions` record gets an optional `total_length` that `stream_text` passes to it. The receiver then reports progress against it and fails the stream if the length does not match.
- Add `close()` to the `core-modules` text and byte stream readers (`closeStream()` in Kotlin) and core `StreamCloser` handles returned by `closer()` to end pending reads with `DataStreamError::AlreadyClosed` and remove streams exactly once; dropping a reader has the same effect.

### Fixes

- An incoming single-packet (inline) data stream whose payload is shorter or longer than the length in its header now fails with `StreamError::Incomplete` or `StreamError::LengthExceeded`, as a chunked stream already does. Before, it ended successfully.
- A data stream that its sender closes early with a reason now fails with that reason (`StreamError::AbnormalEnd`), as in JS, even when its header declared a total length; before, the reader got `StreamError::Incomplete`.

## 0.1.9 (2026-10-20)

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

## 0.1.8 (2026-10-12)

### Fixes

- In-flight incoming data streams now error with `AbnormalEnd` when the room closes. Before, the stream readers ended cleanly, so `read_all()` returned truncated content as if it were complete.

#### `livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,

`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.

## 0.1.7 (2026-09-24)

### Fixes

- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.1.6 (2026-09-22)

### Fixes

- Fix reliable data channel replay: keep the full retry buffer across resumes, drop duplicate reliable packets, and ignore replayed chunks on uncompressed streams instead of failing with `MissedChunk`.

## 0.1.5 (2026-09-09)

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

## 0.1.4 (2026-09-08)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)
- Add the `PASSTHROUGH` encoding preset and remove the unused `UpdateEgressRequest` from the generated protocol

## 0.1.3 (2026-08-25)

### Fixes

- fix(uniffi): register the Bytes custom type once, in livekit-common - #1343 (@pblazej)

## 0.1.2 (2026-08-03)

### Fixes

- Move the concept of "internal" data streams into `livekit` crate from `livekit-data-stream` - #1304 (@1egoman)

## 0.1.1 (2026-07-29)

### Features

- Add data streams v2 - #1192 (@1egoman)
