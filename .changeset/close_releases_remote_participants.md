---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Fix remote participants leaking when a room is closed.

Closing a room dropped its remote participant map without unregistering the callbacks each
remote publication installs. Those callbacks hold the participant that owns them, so every
remote participant was kept alive by a cycle through its own publications — and each one
holds the RTC engine, pinning the peer connections and the WebRTC runtime with it. Closing
a room now tears those participants down, leaving the event stream unchanged.
