---
livekit-api: patch
---

`livekit-api` gains a `public_api` module (behind `services`): a client for the
LiveKit Cloud Public API generated from its OpenAPI spec, with `PublicApi` for
threads and databases. reqwest moves to 0.13; with rustls its TLS config is built
on ring with the same webpki or native roots as before.
