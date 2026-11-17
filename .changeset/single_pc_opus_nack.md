---
livekit: patch
---

Negotiate Opus NACK on every audio section in single peer connection mode, including the initial join offer and send-only audio published before any subscriptions, enabling retransmission of lost audio in both publish and subscribe directions when RED is disabled, such as with end-to-end encryption.
