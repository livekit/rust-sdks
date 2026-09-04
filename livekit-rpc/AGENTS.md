# AGENTS.md

## Architectural overview

- This crate encapsulates the business logic and public APIs for [RPC](https://docs.livekit.io/home/client/data/rpc/): performing calls to a remote participant and handling incoming invocations.
- Not for direct consumption by developers
- Unlike most SDK features which live directly in the [`livekit`](../livekit/) crate, RPC is intentionally isolated here for several reasons:
  - Enforces decoupling from other components (e.g., data channel, RTC engine, signaling client)
  - Enables fast unit testing without linking `libwebrtc`
  - Enables shared implementation amongst multiple _consumers_:
    - [`livekit`](../livekit/): Rust client SDK
    - [`livekit-uniffi`](../livekit-uniffi/): will eventually power downstream client SDKs such as Swift and Kotlin

## Client vs. server split

- The crate is organized into two halves:
  - `client.rs`: outgoing calls (the caller side) — `RpcClientManager`
  - `server.rs`: incoming invocations (the handler side) — `RpcServerManager`
- The two halves never communicate with each other or share state
- Shared types live at the crate root (`types.rs`, `constants.rs`) rather than inside either side

## The transport seam

- This crate never touches a data channel, a peer connection or a signaling client. Everything
  outbound goes through the `RpcTransport` trait in `transport.rs`.
- `livekit` supplies the production implementation (`SessionTransport`, in
  `livekit/src/room/rpc_transport.rs`); `src/tests.rs` supplies `MockTransport`.
- Transport failures surface as `RpcTransportError`, a message-only newtype. The concrete engine
  error type stays in the `livekit` crate. This mirrors `livekit_data_stream::api::SendError`.
- Remote participant lookups (used to choose the v1 or v2 wire format) go through
  `livekit_common::RemoteParticipantRegistry`, which `RpcTransport` requires as a supertrait.

## Two public modules

- `api`: public APIs re-exported by _consumers_ and surfaced to end users
- `backend`: managers and supporting types used internally by _consumers_
- Anything not needed by a consumer stays private to the crate. In particular the RPC version
  constants and the `lk.rpc_request_*` stream attribute keys are implementation details.

## Wire formats

- **v1** sends `RpcRequest` / `RpcResponse` / `RpcAck` as reliable data packets.
- **v2** sends requests and success responses as text data streams on the `lk.rpc_request` and
  `lk.rpc_response` topics, with metadata in stream attributes.
- v2 is selected when the destination advertises `client_protocol >= CLIENT_PROTOCOL_DATA_STREAM_RPC`.
- **ACKs and error responses always use v1 packets**, even in a v2 exchange.
