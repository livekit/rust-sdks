---
livekit-signaling: patch
---

A requested `close` or `restart` no longer emits `SignalEvent::Close`. A join or reconnect response timeout now closes the WebSocket instead of leaving it open.
