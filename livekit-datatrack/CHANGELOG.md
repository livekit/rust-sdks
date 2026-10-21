# Changelog
## 0.2.4 (2026-10-20)

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

## 0.2.3 (2026-10-12)

### Fixes

#### `livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,

`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.

## 0.2.2 (2026-10-07)

### Fixes

- Increase the local and remote data track event buffers so a burst of track lifecycle events cannot fill the channel and deadlock room disconnect.

## 0.2.1 (2026-09-24)

### Fixes

- Upgrade to prost 0.14 across the whole project - #1447 (@1egoman)

## 0.2.0 (2026-09-22)

### Breaking Changes

#### `EncryptionError::Failed` and `DecryptionError::Failed` carry a `reason` string and are no longer `flat_error`,

so a foreign `EncryptionProvider` or `DecryptionProvider` returning an error no longer aborts the process with
"Can't lift flat errors" -- a failed data track decrypt (no E2EE manager, key mismatch, corrupt frame) now 
drops the frame and leaves the room connected.

### Fixes

- Cleanup unused dependencies

## 0.1.15 (2026-09-08)

### Features

- Removes livekit-runtime and converts this package to be tokio only again - #1375 (@1egoman)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)
- Add the `PASSTHROUGH` encoding preset and remove the unused `UpdateEgressRequest` from the generated protocol

## 0.1.14 (2026-08-25)

### Fixes

- Register the `Bytes` UniFFI custom type once in `livekit-common` and borrow it from each component with `uniffi::use_remote_type!`, so the converter is emitted once and the post-generation Swift workaround is no longer needed

## 0.1.13 (2026-07-29)

### Fixes

- Add data streams v2 - #1192 (@1egoman)

## 0.1.12 (2026-07-27)

### Features

- Data tracks schema metadata support.

## 0.1.11 (2026-07-14)

### Fixes

- Make some fields public for data track types
- Refactor data tracks E2EE interface
- Use concrete type for data track manager output events

## 0.1.10 (2026-07-09)

### Fixes

- Handle data track SID reassignment
- introduce LiveKitAPI construct, added smoke tests - #1220 (@davidzhao)

## 0.1.9 (2026-06-23)

### Fixes

- Upgrade protocol to v1.48.0

## 0.1.8 (2026-05-29)

### Fixes

- bump protocol to v1.46.4 - #1121 (@lukasIO)

## 0.1.7 (2026-05-21)

### Features

- Introduce pipeline options for remote data tracks, support multiple in-flight frames.

### Fixes

- Fix compilation error in depacketizer test by using correct variable name.

## 0.1.6 (2026-05-18)

### Fixes

- Add AGENTS.md and minor doc revisions
- Add `cargo-fuzz` target for packet deserialization

## 0.1.5 (2026-05-11)

### Fixes

- Upgrade protocol to v1.45.8

## 0.1.4 (2026-04-23)

### Fixes

- Fix data track packet format issue breaking E2EE

## 0.1.3 (2026-04-02)

### Features

- Rename type to `DataTrackStream`

## 0.1.2 (2026-03-31)

### Fixes

- Upgrade to thiserror 2

## 0.1.1 (2026-03-22)

### Features

- Initial release.
