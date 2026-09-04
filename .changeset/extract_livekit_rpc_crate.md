---
livekit-rpc: patch
livekit: minor
livekit-ffi: patch
---

Moves the RPC implementation into a new `livekit-rpc` crate, alongside the existing
`livekit-data-stream` and `livekit-datatrack` crates.

**Breaking:** the `livekit::rpc` module is gone. The RPC types it held are unchanged and
still re-exported from `livekit::participant` and the prelude, so most code needs no edit;
code that spelled the module out (`use livekit::rpc::RpcError;`) should import from
`livekit::participant` or the prelude instead. `RpcClientManager`, `RpcServerManager` and
`HandleRequestOptions` remain reachable under `livekit::participant` but are now
`#[doc(hidden)]`: they are internal SDK API and were never usable without the (private)
transport trait.

Within the `livekit` crate itself, RPC types are now imported from `livekit-rpc` directly
rather than through those re-exports.

The new crate does not depend on `libwebrtc`, so its unit tests run without building WebRTC.
The transport seam that made this possible was already in place; the only change to it is
that `RpcTransport::publish_data` now returns a message-only `RpcTransportError` instead of
`livekit::RoomError`, mirroring `livekit_data_stream::api::SendError`.

Also fixes four latent bugs found while moving the code:

- An RPC call to a participant who disconnects mid-call now fails promptly with
  `RecipientDisconnected`. Pending calls were never purged on disconnect, so the caller
  waited out its full response timeout (15s by default) and got `ResponseTimeout` instead.
- A server reporting a version that is not valid semver no longer panics the calling task.
  An unparseable version is no longer treated as evidence that the server is too old.
- A v1 `RpcResponse` carrying a compressed payload, or no value at all, now fails with an
  `ApplicationError` instead of resolving the caller with an empty successful response.
- Removed an unguarded `unwrap` when building a v1 response packet, by giving the function
  a signature that cannot represent the invalid state.

Also drops the `semver` dependency from `livekit`, which was only used by the RPC client.
