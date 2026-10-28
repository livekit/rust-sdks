---
livekit-session: patch
---

Add the `livekit-session` crate, whose `Session` fetches credentials from a token source, connects a room, projects the agent's state (`AgentState`) into a `watch` channel, and merges chat and transcripts into one message feed, with a README and an `examples/session` text chat.
