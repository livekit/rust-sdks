---
livekit-session: patch
---

Add the `livekit-session` crate, whose `Session` fetches credentials from a token source, connects a room, publishes the platform microphone and plays the agent's audio by default (`SessionOptions::with_platform_audio`), projects the agent's state (`AgentState`) into a `watch` channel, and merges chat and transcripts into one message feed, with a README and an `examples/session` voice and text chat.
