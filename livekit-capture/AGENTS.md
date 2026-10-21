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
