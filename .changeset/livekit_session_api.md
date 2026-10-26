---
livekit-session: minor
---

Add `Session`, which fetches credentials from a token source, connects a room, projects the agent's state into a `watch` channel, and merges chat and transcripts into one message feed with `send_text`, `messages`, `restore_messages` and `subscribe`.
