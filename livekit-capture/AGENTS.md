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
