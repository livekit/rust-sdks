---
livekit-thread: patch
---

Add the `livekit-thread` crate for Agent 2.0 threads: a `ThreadDirectory` lists, creates and opens the threads of one scope, a `Thread` sends text to the agent's text endpoint over A2A and streams the answer into one message feed, and a `Call` wraps a `Session` that continues the same thread, with its transcripts in the thread's feed.
