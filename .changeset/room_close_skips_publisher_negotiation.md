---
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

`Room::close` no longer logs "failed to negotiate the publisher": it unpublishes the local tracks without renegotiating the publisher it is about to close, and a publisher negotiation still in flight when the session closes now stops without logging an error.
