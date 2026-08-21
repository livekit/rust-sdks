---
livekit-api: patch
livekit: patch
livekit-ffi: patch
livekit-signaling: patch
---

Moves the signalling client into a new `livekit-signaling` crate. livekit-api
re-exports it under the historical `livekit_api::signal_client` path, now marked
deprecated: it is internal SDK API, and dependents should use livekit-signaling
directly. livekit-api no longer depends on livekit-net.

Also drops two dependencies that were declared but never used: `scopeguard` and
`bytes`.
