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
