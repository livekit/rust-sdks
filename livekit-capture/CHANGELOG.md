## 0.1.2 (2026-09-18)

### Features

- Add a capture source that renders a wall clock on the GPU.
- Add a capture source that renders a test pattern on the GPU.

### Fixes

- Automatic capture thread join on drop
- Stop answering a server-initiated Leave (room deleted, duplicate identity) with a client Leave. The server has already ended the session and is closing the signalling socket, so the reply only ever produced the warning "dropping pass-through signal — no stream available" on every such disconnect.

#### Report the reconnect reason to the server when resuming.

Resumes previously sent no reason, so server-side telemetry could not attribute why Rust
clients reconnect — every resume looked like `RR_UNKNOWN`. The engine now records what caused
the episode (signal disconnected, publisher failed, subscriber failed) and reports it on each
resume attempt. The v0 signalling path was also missing the `reconnect_reason` query parameter
entirely, so it would not have been reported even if a reason had been supplied.

#### Fix resume reporting success for a PeerConnection that had not recovered.

A resume decided recovery from `PeerConnectionState`, which keeps reading `Connected` for tens
of seconds after the far end goes away. A resume could therefore emit `Resumed` — and so
`RoomEvent::Reconnected` with `ConnectionState::Connected` — for a session whose subscriber
transport was dead, leaving applications with no signal that they had stopped receiving media.
A resume now requires each transport to have entered `Connected` since the resume began, or to
have held it throughout, rather than trusting the state it currently reports.

## 0.1.1 (2026-09-10)

### Features

- Add a `livekit-capture` crate with the core abstractions for capturing video and publishing it to a LiveKit track.
