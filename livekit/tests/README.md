# Integration Tests

Some test cases depend on a LiveKit server and thus are not enabled by default;
to run them, start a local LiveKit server in development mode, and enable the
E2E test feature:

```sh
livekit-server --dev
cargo test --features __lk-e2e-test
```
