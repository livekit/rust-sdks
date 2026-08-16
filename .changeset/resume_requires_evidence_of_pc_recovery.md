---
livekit: patch
---

Fix resume reporting success for a PeerConnection that had not recovered.

A resume decided recovery from `PeerConnectionState`, which keeps reading `Connected` for
tens of seconds after the far end goes away. A resume could therefore emit `Resumed` — and
so `RoomEvent::Reconnected` with `ConnectionState::Connected` — for a session whose
subscriber transport was dead, leaving applications with no signal that they had stopped
receiving media. Resumes now require evidence of recovery: a negotiation completed since the
resume began, or a connection that never broke.
