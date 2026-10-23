---
livekit: patch
---

Ignore a duplicated server answer. `PeerTransport::set_remote_description` applied every answer it was given, so a second copy — one duplicated by a proxy, or a server retry — was applied again with no local offer left to answer. libwebrtc rejects that (`Failed to set remote answer sdp: Called in wrong state: stable`), and `rtc_session` propagates the error with `?` out of the signal handler, failing a renegotiation that had in fact succeeded.

Offers are not numbered on this transport, so the signaling state is what says whether an answer is still wanted: in `Stable` there is none pending. An offer deferred from the JOIN leaves the connection `Stable` until it is applied, so that counts as a pending offer and its answer is still accepted.

This matches the JavaScript SDK (livekit/client-sdk-js#2114).
