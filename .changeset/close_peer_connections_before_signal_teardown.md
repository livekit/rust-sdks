---
livekit: patch
livekit-ffi: patch
---

# Close peer connections before awaiting signal teardown

`SessionInner::close` released the peer connections only after two awaits that can block
indefinitely, so cancelling `close()` — for example by wrapping it in a timeout — left the
transports open and their ICE UDP sockets bound for the lifetime of the process. Long-lived
clients eventually exhausted their file descriptors. The transports are now closed before
the first await, which makes the teardown safe to cancel.
