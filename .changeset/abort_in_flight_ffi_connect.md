---
livekit-ffi: patch
---

# Abort in-flight FFI connect on disconnect

Cancelling `room.connect()` from a language binding left `Room::connect` running,
so ICE UDP sockets leaked (rust-sdks#1334: a completed Room dropped without
`close()` keeps `engine_task`/`room_task` alive) and a later ReadyFor timeout
sent Panic into the host (python-sdks#785). Connect now allocates an abortable
handle immediately; DisconnectRequest cancels the handshake, joins the connect
task, and `close()`s any Room that already completed. ReadyFor on that early
handle is accepted during handshake. Missed ReadyFor fails that room instead of
panicking the process.
