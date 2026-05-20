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

## Documenting changes

- Changes are documented using [_knope_](https://knope.tech)
- Every PR needs a changeset
- Changeset must list any crates which need to be bumped stemming from the change
- Document changes interactively from the CLI with `knope document-change` or create manually in `/.changeset`
