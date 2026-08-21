---
livekit-api: patch
---

Moves access-token generation and verification into a new `livekit-token` crate.
`livekit_api::access_token::*` continues to resolve to the same types via a
re-export, so no consumer changes are needed.

Also fixes the `services-tokio` and `services-async` features, which used the
access-token types without declaring the `access-token` feature. Building with
`--no-default-features --features services-tokio` previously failed to compile.
