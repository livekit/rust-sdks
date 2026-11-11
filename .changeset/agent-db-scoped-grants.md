---
livekit-token: patch
livekit-api: patch
livekit-protocol: patch
---

Scope agent database access to a thread or a database: `AgentThreadGrants` gains `sql`, the new `agentDatabase` claim (`AgentDatabaseGrants`) grants standalone databases, and `AgentGrants::database_admin` is removed. A thread's database id is its thread id, so `Thread::database_id` is gone.
