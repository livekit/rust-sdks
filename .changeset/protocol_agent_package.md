---
livekit-protocol: patch
livekit-capture: patch
livekit-common: patch
livekit-data-stream: patch
livekit-datatrack: patch
livekit-rpc: patch
---

`livekit-protocol` carries the `livekit.agent` package (`AgentSessionEvent`,
`SessionRequest`, the EOT inference messages), the agent threads messages
and the agent-db service; the crates that depend on it follow its bump.
