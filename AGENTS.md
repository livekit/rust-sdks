# AGENTS.md

## API changes

- Breaking public API changes are to be avoided unless necessary to complete your task
  - Be explicit when you plan to make breaking API changes
- When introducing new API surface, always default to private or `pub(crate)` unless there is a specific reason to expose publicly
- Introduce new public APIs sparingly
- New APIs should have idiomatic doc comments
  - All new functions and types should have at least a one-line description
  - Stay concise and elaborate only when necessary to document unexpected behavior or requirements
  - Use intra-doc links
- Include doc tests when doing so meaningfully clarifies API usage
  - Design to be runnable (i.e., no `ignore`)
  - Hide lines that are only for setup from generated docs by prefixing with `#`

## Dependencies

- When using APIs from third-party crates, never assume you already know the API
  - **ALWAYS** reference [_docs.rs_](https://docs.rs) for docs of the specific version being used
- Pull in new dependencies as a last resort
  - Explore docs of existing dependencies first to discover new APIs that can be leveraged to get the desired behavior

## Design patterns & conventions

- Functions should generally accept `&str`/`&[T]` rather than `String`/`Vec<T>`
- Generally avoid clones, but pay special attention when cloning in a high-frequency code path
  - In such cases, reach for smart pointers (e.g., `Arc<T>`) instead
- When porting code from other languages, avoid mechanical translation
  - Apply Rust-specific design patterns and conventions when doing so improves readability or safety
  - Some patterns common in other languages are heavily discouraged in Rust (e.g., singleton)
- Leverage the [new type pattern](https://doc.rust-lang.org/rust-by-example/generics/new_types.html) where applicable
- Implement `From`/`TryFrom` for performing conversion between types
- Avoid large, catch-all error enums for new APIs
  - Prefer smaller, context-scoped enums whose variants are limited to errors that can actually occur at that call site
- Prefer the actor pattern for async tasks
  - Model as a struct encapsulating local state with an async, consuming run method
  - Other methods can operate on `&self` to keep `run` small
- The `livekit` crate is designed to be async runtime agnostic

## Safety

- Avoid `unwrap` except in tests
  - When unavoidable, prefer `expect` instead and provide a concise message explaining what went wrong (e.g., "Invalid state")
- Avoid `unsafe` unless absolutely necessary
  - Typically will only be used in an FFI context (i.e., in `webrtc-sys`)
- When unavoidable, follow these guidelines
  - Wrap unsafe code in a safe function or struct
  - Isolate only the unsafe operations
  - Every unsafe block should have a `// SAFETY:` comment explaining why the operation is actually safe (e.g., verifying pointers are non-null)

## Style guidelines

- Always format using `cargo fmt`
- Avoid excessive nesting and prefer [`let-else`](https://doc.rust-lang.org/rust-by-example/flow_control/let_else.html)
- Avoid long parameter lists; group related inputs into a purpose-built struct when it improves readability

## UniFFI integration

Several crates export items to Swift/Kotlin/Node/Python through UniFFI — `livekit-uniffi`, plus `livekit-common`, `livekit-datatrack`, and `livekit-net`, each carrying its own `uniffi.toml`. The Kotlin bindgen has sharp edges that `cargo build`, `cargo test`, and the Swift/Node/Python bindings do **not** catch: they surface only when the generated Kotlin is compiled, and one bad name fails the entire generated file.

- Verify any change to UniFFI-exported API by actually generating and compiling the Kotlin bindings (`cargo make android-package` from `livekit-uniffi/`) — a green `cargo build` proves nothing here
- **Never export a method named `close`**
  - UniFFI gives every object a non-`suspend` `close()` to satisfy `AutoCloseable`. An exported Rust method also called `close` differs from it only by `suspend`, which Kotlin rejects as conflicting overloads — see [mozilla/uniffi-rs#2955](https://github.com/mozilla/uniffi-rs/issues/2955)
  - Work around it with a Kotlin-only rename, so the Rust source and the Swift/Node/Python bindings keep the original name:
    ```toml
    [bindings.kotlin.rename]
    "ByteStreamWriter.close" = "close_stream"
    ```
  - The rename must go in the `uniffi.toml` of the crate that **declares** the item, not in `livekit-uniffi/uniffi.toml`: a rename table only reaches items from the crate that owns it. `WsConnection.close` is renamed in `livekit-net/uniffi.toml` for exactly this reason
- **Never name a field of an exported enum or record `message`**
  - For an error variant carrying a `message` field, UniFFI emits a constructor property `message` next to an `override val message` inherited from `Throwable` in one class body, which does not compile — and their types differ (`String` vs `String?`), so they cannot be merged into a single override. See [mozilla/uniffi-rs#2938](https://github.com/mozilla/uniffi-rs/issues/2938), closed without a fix
  - Unlike `close`, this **cannot** be renamed away: UniFFI keys the rename table by crate name but looks up enum and record members by the item's full module path, so a rename for anything declared in a submodule is silently ignored (method renames use the crate name and do work)
  - Name the field `reason` in Rust instead — see `DataStreamError` in `livekit-uniffi/src/data_stream/common.rs`
- A new crate that exports UniFFI items needs its own `uniffi.toml`, including `omit_checksums = true` under `[bindings.kotlin]`
  - The Kotlin checksum test is broken on ARM in every UniFFI release this workspace can use; the full explanation lives in `livekit-uniffi/uniffi.toml` and the root `Cargo.toml`

## Feature combinations

`.github/workflows/feature-combinations-curated.yml` is the only job in CI that exercises features in *combination*. A `dep:` that one feature pulls in but the code uses unconditionally, or a `?/` forward that silently no-ops, compiles fine under default features and breaks only for the caller who picks a particular set — nothing else catches that. It has two halves, maintained differently, and both need updating by hand.

- **When adding a feature to `livekit` or `livekit-api`**, add the configurations a user would plausibly select to the `COMBOS` list in the "Top-level crates" step
  - These two crates get a hand-picked list rather than a powerset, so a new feature is invisible to this job until it is listed there
  - One line per configuration, `crate|cargo feature flags`; an empty flags field means default features
  - Pair the feature with what it realistically ships alongside (a TLS backend together with `native`, say) rather than listing it on its own
  - Do not add combinations nobody can select — internal `__lk-*` flags and two-TLS-backends-at-once are deliberately absent, and were most of what made the full powerset expensive
- **When adding a workspace crate below `livekit`/`livekit-api`**, add `-p <crate>` to `LEAF_PACKAGES`
  - Leaf crates get the full `cargo hack --feature-powerset --depth 2`, so listing the crate is all that is needed; its features are then picked up automatically as they are added
  - cargo-hack only varies the features of packages named with `-p`. An unlisted crate still gets built as a dependency, which makes it easy to assume it is covered when it is not
- Keep `--depth 2`. `livekit` alone goes from 67 combinations to 232 at depth 3, and pairwise interactions are where these bugs actually live
- A feature that pulls in `openssl-sys` cannot build for the three Android targets — there is no Android OpenSSL to link against. Add it to their `exclude_features` rather than trying to make it work; Android ships rustls (see `ffi-builds.yml`)
- `livekit-ffi` and `livekit-uniffi` are excluded on purpose: their combinations cost more than everything else in the job combined, because each TLS change rebuilds `livekit` underneath them. Their shipped configurations are covered by `builds.yml` and `ffi-builds.yml` instead
- Check a change to either list without building anything by appending `--print-command-list`, which enumerates the combinations cargo-hack would run

## Documenting changes

- Changes are documented using [_knope_](https://knope.tech)
- Every PR needs a changeset
- Changeset must list any crates which need to be bumped stemming from the change
- Document changes interactively from the CLI with `knope document-change` or create manually in `/.changeset`
