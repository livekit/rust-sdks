---
livekit: patch
livekit-api: patch
---

Fix compile failures caused by changes in the `livekit-protocol` schema, specifically by initializing new `compression` and `inline_content` fields in the outgoing `Header` struct, and the new `media` field in SIP inbound/outbound trunks.
