---
livekit: patch
livekit-ffi: patch
---

Fix a publisher-transport deadlock during renegotiation. When another negotiation was requested while an offer was awaiting its answer, `set_remote_description` re-entered `create_and_send_offer` while holding the transport's non-reentrant inner mutex, permanently wedging publishing.
