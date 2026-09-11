# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.3 (2026-09-11)

### Fixes

#### `VideoGrants` gains the `agent` grant and `Claims` the `kind` claim (with

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

## 0.1.2 (2026-09-09)

### Fixes

- log warning if signal messages get dropped during reconnect - #1391 (@lukasIO)

## 0.1.1 (2026-09-08)

### Features

- Removes livekit-runtime and converts this package to be tokio only again - #1375 (@1egoman)

### Fixes

- Add data streams v2 to exposed uniffi interface - #1286 (@1egoman)

#### Moves the signalling client into a new `livekit-signaling` crate. livekit-api

re-exports it under the historical `livekit_api::signal_client` path, now marked
deprecated: it is internal SDK API, and dependents should use livekit-signaling
directly. livekit-api no longer depends on livekit-net.

Also drops two dependencies that were declared but never used: `scopeguard` and
`bytes`.

## 0.1.0

The initial release. This was broken out of [livekit-api](../livekit-api) into its own crate.
