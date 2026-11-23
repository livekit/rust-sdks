---
livekit: patch
livekit-api: patch
livekit-capture: patch
livekit-common: patch
livekit-data-stream: patch
livekit-datatrack: patch
livekit-ffi: patch
livekit-protocol: patch
livekit-rpc: patch
livekit-signaling: patch
livekit-telemetry: patch
livekit-token: patch
---

Add token mode to the server API clients. `LiveKitApi::with_token_provider` asks a `TokenProvider` for a token with the grants of each request, so a client can run without an API key and secret. `Claims::to_claims_json` gives the grants in the form that a job token request takes.
