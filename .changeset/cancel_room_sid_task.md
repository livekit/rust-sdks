---
livekit-ffi: patch
---

Cancel and join the room SID notification task on disconnect so closing a room before its SID arrives does not retain the room and its WebRTC state.
