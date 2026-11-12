---
livekit-token: patch
livekit-api: patch
livekit-protocol: patch
livekit: patch
livekit-capture: patch
livekit-common: patch
livekit-data-stream: patch
livekit-datatrack: patch
livekit-ffi: patch
livekit-rpc: patch
livekit-signaling: patch
---

Scope agent database access to a thread or a database: `AgentThreadGrants` gains `sql`, and the new `agentDatabase` claim (`AgentDatabaseGrants`) grants `read`, `write` and `delete` on listed databases plus project-wide `create` and `list`. `AgentGrants::database_admin` is removed, and `DatabaseClient` signs each call with only the grant it needs. A thread's database id is its thread id, so `Thread::database_id` is gone.
