# AGENTS.md

## Architectural overview

- This crate encapsulates the business logic and public APIs for the [data tracks feature](https://docs.livekit.io/transport/data/data-tracks/)
- Not for direct consumption by developers
- Unlike most SDK features which live directly in the [`livekit`](../livekit/) crate, data tracks are intentionally isolated here for several reasons:
  - Enforces decoupling from other components (e.g., data channel, signaling client, etc.)
  - Enables proper integration testing
  - Enables shared implementation amongst multiple _consumers_:
    - [`livekit`](../livekit/): Rust client SDK
    - [`livekit-uniffi`](../livekit-uniffi/): will eventually power downstream client SDKs such as Swift and Kotlin

## Local vs. remote split

- The crate is organized into two structurally parallel halves:
  - `local/`: publishing
  - `remote/`: subscribing
- Each side has its own `manager`, `events`, `pipeline`, and `proto` modules with matching shape
- The symmetry is deliberate; when changing behavior on one side, look for and consider the mirror on the other
- The two halves never communicate with each other or share state
- Shared types (`DataTrackInfo`, `DataTrackFrame`, etc.) live at the crate root rather than inside either side

## Boundaries

- Two public modules get exported from this crate:
  1. `api`: public APIs that get be re-exported by _consumers_ and made available to developers
  2. `backend`: managers and supporting types used internally by _consumers_ to power the feature
- Events handled by the managers are decoupled from protocol messages for several reasons:
  - Protobuf is a wire format and cannot express Rust-level invariants
  - Events can carry in-process types proto cannot (e.g., `oneshot::Sender` for in-actor request/response)
  - Allows the protocol to evolve independently
- Consequently:
  - The `proto` modules are the sole import sites for `livekit_protocol`
  - New wire conversions go there and `livekit_protocol` must not be referenced anywhere else
