# AGENTS.md

## Architectural overview

- This crate encapsulates the business logic and public APIs for the data streams feature: [text streams](https://docs.livekit.io/transport/data/text-streams/) and [byte streams](https://docs.livekit.io/transport/data/byte-streams/)
- Not for direct consumption by developers
- Unlike most SDK features which live directly in the [`livekit`](../livekit/) crate, data streams are intentionally isolated here for several reasons:
  - Enforces decoupling from other components (e.g., data channel, signaling client, etc.)
  - Enables proper integration testing
  - Enables shared implementation amongst multiple _consumers_:
    - [`livekit`](../livekit/): Rust client SDK
    - [`livekit-uniffi`](../livekit-uniffi/): will eventually power downstream client SDKs such as Swift and Kotlin

## Incoming vs. outgoing split

- The crate is organized into two halves:
  - `incoming/`: receiving streams from remote participants (produces readers)
  - `outgoing/`: sending streams to remote participants (produces writers)
- The two halves never communicate with each other or share state
- Shared types live at the crate root rather than inside either side:
  - wire/domain packet types (`Header`, `Chunk`, `Trailer`, `Packet`, `StreamId`, ...) in `types/`
  - `ByteStreamInfo` / `TextStreamInfo` in `info.rs`
  - `StreamError`, `StreamResult`, `StreamProgress`, `SendError` in `utils.rs`
  - helpers such as UTF-8-aware chunking in `utf8_chunk.rs`

### The halves are deliberately *not* symmetric

Unlike the mirror-image `local/`/`remote/` split in [`livekit-datatrack`](../livekit-datatrack/AGENTS.md), `incoming/` and `outgoing/` do **not** share a parallel shape. Each has a `manager`, but they are built differently on purpose:

- The **incoming** manager (`incoming/manager.rs`) is an **actor**. It owns all receive-side state on a single task and is driven by `InputEvent`s fed over a channel, emitting `OutputEvent`s for the host to surface (see `incoming/events.rs`). This is required because inbound packets arrive from the engine's event loop in a context that cannot `.await` into the manager, inbound chunks must never be dropped (a dropped chunk is an unrecoverable `MissedChunk`), and processing must not head-of-line-block that loop. Owning its state directly also lets its handlers `.await` decompression on the run-loop task without holding a lock across the await point.
- The **outgoing** manager (`outgoing/manager.rs`) is a plain struct with `async` methods that callers `.await` directly. It runs in an already-async context (publishing), so awaiting into it is fine and the actor machinery (events, channels, a run loop) would add substantial complexity for no benefit.

This asymmetry is a conscious choice, **not** an oversight or unfinished work. Do not "symmetrize" the outgoing side into an actor preemptively. When changing behavior on one side, there is often no mirror to update on the other — verify rather than assume.

## Boundaries

- Two public modules get exported from this crate (see `lib.rs`):
  1. `api`: public APIs that get re-exported by _consumers_ and made available to developers (readers, writers, stream options, stream infos, `StreamError`, ...)
  2. `backend`: managers and supporting wire types used internally by _consumers_ to power the feature (`backend::incoming`, `backend::outgoing`, and the domain packet types)
- The incoming actor's events are decoupled from protocol messages for several reasons:
  - Protobuf is a wire format and cannot express Rust-level invariants
  - Events can carry in-process types proto cannot (e.g., the channel senders used to feed a reader)
  - Allows the protocol to evolve independently
- Wire <-> domain conversion lives in `types/packet.rs`, which owns the `From`/`TryFrom` impls between `livekit_protocol::data_stream` messages and this crate's own packet types. Consumers convert inbound proto into domain `Packet`s before feeding the incoming actor, so the incoming side only ever sees domain types.
