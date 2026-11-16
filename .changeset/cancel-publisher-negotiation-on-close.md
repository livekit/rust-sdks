---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Cancel and join pending publisher negotiations when a room closes, including the debounce wait, so negotiation tasks no longer retain peer connection factories, media threads, and encoder resources after teardown.
