---
libwebrtc: major
livekit: patch
---

`OfferOptions::offer_to_receive_audio` and `offer_to_receive_video` are now `Option<bool>`. `None` leaves libwebrtc's option unset, so receive-only transceivers stay receive-only. The default is still `Some(false)`. Single peer connection offers leave both unset and no longer rewrite `a=inactive` to `a=recvonly`.
