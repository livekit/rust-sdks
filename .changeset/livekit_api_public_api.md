---
livekit-api: patch
---

`livekit-api` gains `ThreadClient` and `DatabaseClient` services (`LiveKitApi::threads()` and
`LiveKitApi::databases()`) for agent threads and databases, built on a client generated from
the LiveKit Cloud Public API spec. reqwest moves to 0.13; with rustls its TLS config is built
on ring with the same webpki or native roots as before.
