# Integration Tests

Some test cases depend on a LiveKit server and thus are not enabled by default;
to run them, start a local LiveKit server in development mode, and enable the
E2E test feature:

```sh
livekit-server --dev
cargo test --features default,__lk-e2e-test -- --nocapture
```

Tip: If you are using Rust Analyzer in Visual Studio Code, you can enable this feature to get code completion for these tests. Add the following setting to *.vscode/settings.json*:

```json
"rust-analyzer.cargo.features": ["default", "__lk-e2e-test"]
```
