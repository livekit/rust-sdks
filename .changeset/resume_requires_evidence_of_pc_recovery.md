---
livekit: patch
---

Fix resume reporting success for a PeerConnection that had not recovered.

A resume decided recovery from `PeerConnectionState`, which keeps reading `Connected` for tens
of seconds after the far end goes away. A resume could therefore emit `Resumed` — and so
`RoomEvent::Reconnected` with `ConnectionState::Connected` — for a session whose subscriber
transport was dead, leaving applications with no signal that they had stopped receiving media.
A resume now requires each transport to have entered `Connected` since the resume began, or to
have held it throughout, rather than trusting the state it currently reports.
