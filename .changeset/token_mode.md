---
livekit-api: minor
livekit-protocol: minor
livekit-token: patch
---

Add token mode to the server API clients. `LiveKitApi::with_token_provider` asks a `TokenProvider` for a token with the grants of each request, so a client can run without an API key and secret. `Claims::to_claims_json` gives the grants in the form that a job token request takes.

`ServiceError` is now `#[non_exhaustive]`, and has the new variants `TokenProvider` and `InvalidToken`. A `match` on it needs a wildcard arm.

A thread call now asks for one action on the thread or scope that it names, in place of every action on every thread. For example, `ThreadClient::get("AT_1")` asks for `{"read": true, "threadIds": ["AT_1"]}`.
