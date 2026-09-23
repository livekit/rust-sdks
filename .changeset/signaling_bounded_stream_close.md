---
livekit-signaling: patch
---

Never wait on the peer when closing a signal stream: a half-open socket (no FIN/RST) left the reader parked in `recv()` forever, so the resume never dialled and `Room::close` hung. `SignalStream::close` now aborts the parked reader and bounds the writer's Close flush.
