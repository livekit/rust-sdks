## 0.1.2 (2026-09-14)

### Fixes

- Automatic capture thread join on drop
- Stop answering a server-initiated Leave (room deleted, duplicate identity) with a client Leave. The server has already ended the session and is closing the signalling socket, so the reply only ever produced the warning "dropping pass-through signal — no stream available" on every such disconnect.

## 0.1.1 (2026-09-10)

### Features

- Add a `livekit-capture` crate with the core abstractions for capturing video and publishing it to a LiveKit track.
