---
livekit-signaling: patch
---

# Bound `SignalStream::close` so a dead signal link cannot hang `Room::close()`

A signal WebSocket can stop delivering data while the socket stays open — no FIN, no
RST, no ICMP error, which is what a lost cellular or Wi-Fi uplink looks like. The read
task waits in `conn.recv()`, and nothing could wake it: `NativeConnection::close` closes
the writer only, and the reader keeps polling a socket that never delivers. Since
`SignalStream::close` joined the read task's handle, it never returned. That took the
room down with it: the reconnect never reached `SignalStream::connect` because `restart`
was waiting on the stream lock, and `Room::close()` never returned from the `Leave` send.
Devices stayed in a room for hours after the SDK had reported them disconnected.

The read task now selects on a shutdown channel owned by the `SignalStream`, so `close`
can stop it, and dropping a `SignalStream` without closing it stops its tasks too. A
`CLOSE_DRAIN_TIMEOUT` backstop bounds the shutdown if a transport stalls some other way
— a `send` waiting on TCP retransmission, say — and aborts the tasks rather than leaving
them parked on the socket.

`SignalInner::close` also takes the stream out of the slot before closing it, so the
write lock is no longer held across the shutdown.
