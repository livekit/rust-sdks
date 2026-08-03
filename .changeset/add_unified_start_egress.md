---
livekit-api: minor
livekit: patch
livekit-ffi: patch
livekit-uniffi: patch
---

Add a unified `EgressClient::start_egress` that calls the v2 `Egress.StartEgress` RPC with a `StartEgressRequest`, alongside the existing per-type helpers.
