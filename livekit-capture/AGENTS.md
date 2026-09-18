# AGENTS.md

## Adding a source

- Add a feature gate in `Cargo.toml`
  - Name the feature after the source module with a `source-` prefix
    (e.g., module `gstreamer` → feature `source-gstreamer`)
  - If the new source requires dependencies, they should only be include when feature is enabled
- Define a module for the new source in `src/sources/mod.rs`:
  ```rust
  #[cfg(feature = "source-my-source")]
  pub mod my_source;
  ```
- Follow conventions for existing sources
- Source implementation should be self-contained in its module and submodules
  - Only hoist functionality to a higher level module when shared by multiple sources
- Source (e.g, `MyVideoSource`) is constructed from a config struct (e.g., `MyVideoSourceConfig`)
- Add conditional derives for `serde` and `schemars` for config
- Implement `PixelVideoSource` or `EncodedVideoSource` for your source
  - NEVER add a source that is not consumable uniformly through one of these traits
  - Doing so would break API contract and break integration for consumers
- Keep API surface minimal and hide implementation details

## Adding integration tests

- Integration tests verify a source against a real backend (e.g., a real
  RTSP server, a real capture device) and never run by default
- Gate each source's tests behind an internal feature named
  `__test-source-<module>` (e.g., `__test-source-rtsp`) that enables
  `source-<module>` plus any test-only dependencies
  - Test-only dependencies are optional entries in `[dependencies]` enabled
    only by the test feature (Cargo has no optional dev-dependencies)
- Tests live in `tests/source_<module>_test.rs`, with
  `#![cfg(feature = "__test-source-<module>")]` at the top so the target
  compiles to empty without the feature
- Helpers live in `tests/common/`: source-agnostic ones (e.g., pull loops)
  in `tests/common/mod.rs`, and source-specific ones (test servers,
  pipelines) in a `tests/common/<module>.rs` submodule declared with
  `#[cfg(feature = "__test-source-<module>")]`
- Test the source through its public API (construct, `next_access_unit` or
  `next_frame`); do not publish to an RTC source or a LiveKit server
- Launch backends in-process or automatically; a test must not depend on a
  manually started process
- Document host prerequisites and the run command in `tests/README.md`

## Handling untrusted input

All bytes from the network or a device are attacker-controlled. When parsing:

- Never index or slice untrusted bytes with manual cursor arithmetic; read
  through the `Option`-returning readers (`ByteReader`, `BitReader`) so
  bounds-safety is structural
  - Do not use panicking accessors (e.g., `bytes::Buf` getters) on
    untrusted input
- Bound every buffer that grows across packets or messages with an explicit
  cap (e.g., `MAX_PENDING_ACCESS_UNIT_BYTES`), and never let a claimed
  length drive an allocation — allocate only for bytes actually received
- Use checked arithmetic on untrusted lengths and bound varint/loop
  decoders (LEB128, Exp-Golomb) so they cannot spin
- Malformed input is data, not a crash: return a typed error or engage loss
  recovery; keep parser state valid after every error
- Escape or strip untrusted strings before logging them or embedding them
  in error messages (strip control characters; log via `{:?}` or
  `escape_debug`)
- Never let credentials reach logs, error strings, or request URIs; redact
  them in `Debug` implementations
- Insecure opt-outs (e.g., disabled TLS verification) must be explicit
  configuration, documented as such, and warned about at runtime
- Prefer well-maintained crates for standard wire grammars (RTSP, SDP,
  auth, TLS); keep only domain interpretation in this crate
- Keep (de)serialization in small pure functions with unit tests, so they
  stay auditable and fuzzable
