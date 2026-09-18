---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Fix the RTC engine leaking when a room is dropped without being closed.

The engine's event task held a strong reference to the engine, while the signal that stops
that task lives inside the engine itself, so the two kept each other alive. An engine
dropped without an explicit `close()` released nothing: the session, both peer connections,
the WebRTC runtime, and the signal client with its open websocket all stayed resident for
the lifetime of the process, and the server kept its half of the session because the socket
was never closed. The task now holds a weak reference and stops on its own once the engine
is gone.
