---
libwebrtc: minor
livekit: patch
---

Add `PeerConnection::create_offer_with_offer_to_receive_unset`, which leaves libwebrtc's legacy `offer_to_receive_*` options unset so receive-only transceivers stay receive-only. Single peer connection offers use it and no longer rewrite `a=inactive` to `a=recvonly`.
