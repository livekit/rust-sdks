---
livekit: patch
---

Stop answering a server-initiated Leave (room deleted, duplicate identity) with a client Leave. The server has already ended the session and is closing the signalling socket, so the reply only ever produced the warning "dropping pass-through signal — no stream available" on every such disconnect.
