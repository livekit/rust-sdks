---
livekit-protocol: patch
---

`livekit-protocol` exposes the conversation schema of a thread's database, as protocol's Go package
does: `agent::THREAD_SCHEMA_VERSION`, `agent::THREAD_SCHEMA_V1` (the SQL) and
`agent_task::Status::from_task_state`.
