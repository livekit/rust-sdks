---
webrtc-sys: patch
libwebrtc: patch
livekit: patch
livekit-capture: patch
livekit-ffi: patch
---

Release local tracks when the server ends the room.

On a server-initiated disconnect (room deleted, participant removed, duplicate identity) the
engine closed the publisher `PeerConnection` before the room's unpublish loop ran. libwebrtc
refuses `RemoveTrack` on a closed PeerConnection and never releases the sender's track, so
every local track of a server-ended room stayed alive for the life of the process — for a
local audio track, one `AudioSourceCapture` thread per room. The engine now detaches every
publisher sender before closing the PeerConnection, `unpublish_track` completes its
bookkeeping even when the engine has already closed the transport, and
`RtpSender::track()` returns `None` for a detached sender instead of dereferencing null.
