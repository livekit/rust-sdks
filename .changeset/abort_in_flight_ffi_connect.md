---
livekit-ffi: patch
---

# Abort in-flight FFI connect on disconnect

Cancelling `room.connect()` from a language binding left `Room::connect` running,
and a later ReadyFor timeout sent Panic into the host (python-sdks#785). Connect
now allocates an abortable handle immediately; DisconnectRequest cancels the
handshake, joins the connect task, and `close()`s any Room that already
completed (rust-sdks#1334: dropping a completed Room without close keeps
`engine_task`/`room_task` alive). Abort before `Room::connect` returns Ok does
not enter `SessionInner::close`; that leftover stays in `livekit/`. ReadyFor on
that early handle is accepted during handshake. Missed ReadyFor fails that room
instead of panicking the process.
