# AGENTS.md

## Architectural overview

- Holds foundational, broadly-shared items used across multiple LiveKit crates (e.g. [`livekit`](../livekit/), [`livekit-api`](../livekit-api/), [`livekit-data-stream`](../livekit-data-stream/))
- Internal crate — not for direct consumption by developers (public APIs live in the [`livekit`](../livekit/) crate)
- Exists purely to avoid duplication and circular dependencies: an item needed by two or more downstream crates lives here instead of in any single one
- Current contents set the bar for what fits: `ParticipantIdentity` (newtype), `EncryptionType` (enum + proto conversions), the `CLIENT_PROTOCOL_*` constants, and the `enum_dispatch!` macro

## What belongs here

- Small, self-contained, foundational items shared by **two or more** downstream crates:
  - Newtypes and plain data enums (plus their `From`/`TryFrom` conversions)
  - Simple constants
  - Trivial, stateless helper functions and declarative macros
- Every addition must be dependency-light (see Dependencies) and free of feature/business logic

## What does NOT belong here

- Feature or business logic — keep it in the feature's own crate (e.g. `livekit-data-stream`) or in `livekit`
- Items used by only **one** crate — leave them in that crate until a second consumer actually needs them; do not hoist here speculatively
- Stateful components — managers, actors, services, or anything holding runtime state
- Wire/protocol types — those belong in `livekit-protocol` (this crate depends on it, never the reverse)
- Anything that would require a heavy or environment-specific dependency (see Dependencies)

## Dependencies

- Keep the dependency list minimal — today it is only `livekit-protocol`
- A dependency added here is forced onto **every** downstream crate; treat any new dependency as a red flag and justify it explicitly
- Never pull in heavy or environment-specific deps (`libwebrtc`, an async runtime, networking, etc.) — a type that needs those belongs in a higher-level crate
