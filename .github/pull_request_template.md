### Before you submit your PR

Make sure the following is true before submitting your PR:

- [ ] I have read the [contributing guidelines](https://github.com/livekit/rust-sdks/blob/main/CONTRIBUTING.md) and validated that this PR will be accepted.
- [ ] I have read and followed the principles regarding breaking changes, testing, and code quality.

### PR description

Describe the changes in this PR. Explain what the PR is meant to solve and how to reproduce the issue in the first place.

### Breaking changes

If this PR introduces breaking changes, list them here and document the rationale for introducing such a change.

### MSRV

If the PR modifies the crate's MSRV (Minimum Supported Rust Version), document it here.

### Testing

Ideally, unit test the code you add, but ensure you're not repeating existing test cases. Use as many already written scaffolding, utilities as possible; write your own, when needed. If external services, APIs, tokens are required (e.g., running an LK server instance), provide the necessary information. Make sure your tests perform useful, context-aware assertions and do not simply emulate "happy paths".

### Async

We want the project to be runtime-agnostic, so please reuse what's already in [livekit-runtime](https://github.com/livekit/rust-sdks/blob/main/livekit-runtime/) and feel free to add anything missing. It's ok to use Tokio directly, when writing unit tests, if necessary. When testing, do not use artificial delays for the state to "catch up"; instead, respect the event flow and subscribe properly using channels or other mechanisms.


