---
livekit-token: major
livekit-uniffi: major
livekit-api: major
livekit: patch
livekit-ffi: patch
livekit-signaling: patch
---

`VideoGrants` gains the `agent` grant and `Claims` the `kind` claim (with
`AccessToken::with_kind`), which the Go, Python and JS SDKs already carry. An
agent worker's token is `VideoGrants { agent: true }` and a simulated job's
participant token is `kind: "agent"`; neither could be minted from Rust before.
`livekit-uniffi` exposes both: `TokenOptions.kind`, `Claims.kind`, and `agent`
on its `VideoGrants` record.

**Breaking:** the four grants the server infers when absent -- `can_publish`,
`can_subscribe`, `can_publish_data`, `can_update_own_metadata` -- are now
`Option<bool>`, as in the Go and JS SDKs. `None` leaves the decision to the
server, and the new getters (`can_publish()`, `can_subscribe()`,
`can_publish_data()`, `can_update_own_metadata()`) read a token the way the
server does, `can_publish_data` falling back to `can_publish` included. Code
that set these fields writes `Some(..)`; code that read them uses the getters.
The same fields are optional on the `livekit-uniffi` record, and
`livekit-api` re-exports the crate as `livekit_api::access_token`, so both
carry the change.

Nothing at its default is written into the token any more: unset claims and
grants are omitted, as the server's own `omitempty` grants are. Verification
of existing tokens is unchanged.
