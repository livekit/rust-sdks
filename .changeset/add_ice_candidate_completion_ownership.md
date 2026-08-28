---
webrtc-sys: patch
libwebrtc: patch
---

# Keep `add_ice_candidate` completion state alive until libwebrtc completes

The native `PeerConnection::add_ice_candidate` shim passed libwebrtc a lambda that captured
the Rust context and completion function by reference, even though
`PeerConnectionInterface::AddIceCandidate` completes asynchronously. Whenever libwebrtc's
operations chain deferred the completion — for example when a `CreateOffer` or
`SetRemoteDescription` was still in flight — the shim returned first and destroyed both
captures. Dropping the context dropped the pending `oneshot::Sender`, so the Rust future
resolved with `add_ice_candidate cancelled`, and the later callback read freed memory,
crashing with `SIGSEGV`/`SIGBUS`.

The completion state now lives in a shared, single-use object that outlives the call, is
safe for the copies libwebrtc makes of the callback, and hands the context back to Rust
exactly once.
