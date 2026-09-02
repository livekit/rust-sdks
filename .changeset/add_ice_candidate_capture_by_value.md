---
webrtc-sys: patch
---

# Own the add_ice_candidate completion state

`PeerConnection::add_ice_candidate` captured `ctx` and `on_complete` by reference in the
completion lambda it hands to libwebrtc. That completion runs asynchronously on the signaling
thread and is deferred behind the operations chain whenever it is busy, for example while a
`SetRemoteDescription` is in flight, so it could execute after the calling frame had returned
and dereference freed stack memory (a crash on the signaling thread on the first ICE candidate
in practice). The lambda now owns its state through a `shared_ptr`.
