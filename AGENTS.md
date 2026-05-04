# AGENTS.md

## API changes

- Breaking public API changes are to be avoided unless necessary to complete your task
  - Be explicit when you plan to make breaking API changes
- When introducing new API surface, always default to private or `pub(crate)` unless there is a specific reason to expose publicly
- Introduce new public APIs sparingly
- New public types that will be extensively used should be added to the prelude

## Dependencies

- When using APIs from third-party crates, never assume you already know the API
  - **ALWAYS** reference [_docs.rs_](https://docs.rs) for docs of the specific version being used
- Pull in new dependencies as a last resort
  - Explore docs of existing dependencies first to discover new APIs that can be leveraged to get the desired behavior

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
