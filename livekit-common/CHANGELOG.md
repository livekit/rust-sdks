## 0.2.0 (2026-11-18)

### Breaking Changes

- Add `semconv`, the telemetry names and enum values generated from the protocol's Weaver registry, and move the protocol submodule to `538dd3f0`, the merged registry. This is a breaking upgrade of the generated Rust source, taken as a minor bump before 1.0 with no compatibility alias: `DataTrackExtensionId::DteiParticipantSidLow` is gone and value 1 is now `DteiE2ee` (JSON name `DTEI_E2EE`, so the old name no longer parses), `media_source::Video` gains the `Videos` variant, `media_source::Video`, `StreamOutput`, `UpdateStreamRequest` and `MigrateJobRequest` lose `Eq` and `Hash`, `simulation_run::job_metrics::Turn` loses `Copy`, and request messages such as `UpdateStreamRequest` and `TransferSipParticipantRequest` gain fields, so exhaustive matches and struct literals in downstream code need `..Default::default()` or new arms. `livekit-common`, `livekit-data-stream`, `livekit-datatrack`, `livekit-rpc`, `livekit-token`, `livekit-signaling` and `livekit` expose protocol types in their public API, and `livekit-capture` exposes `livekit` types, so each takes the same break.

### Fixes

- AgentDB wire errors carry a numeric code with its name and description, and Hello carries the client version.
- Update dependent SDK packages for the Agents 2.0 protocol and service client changes.

## 0.1.7 (2026-11-13)

### Fixes

- Scope agent database access to a thread or a database: `AgentThreadGrants` gains `sql`, and the new `agentDatabase` claim (`AgentDatabaseGrants`) grants `read`, `write` and `delete` on listed databases plus project-wide `create` and `list`. `AgentGrants::database_admin` is removed, and `DatabaseClient` signs each call with only the grant it needs. A thread's database id is its thread id, so `Thread::database_id` is gone.

## 0.1.6 (2026-10-20)

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

## 0.1.5 (2026-10-12)

### Fixes

#### `livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,

`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.

## 0.1.4 (2026-09-24)

### Fixes

- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.1.3 (2026-09-08)

### Fixes

- Add the `PASSTHROUGH` encoding preset and remove the unused `UpdateEgressRequest` from the generated protocol

## 0.1.2 (2026-08-25)

### Fixes

- Register the `Bytes` UniFFI custom type once in `livekit-common` and borrow it from each component with `uniffi::use_remote_type!`, so the converter is emitted once and the post-generation Swift workaround is no longer needed
