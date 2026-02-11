# Integration Tests

Some test cases depend on a LiveKit server and thus are not enabled by default.

## Basic E2E Tests

To run basic E2E tests, start a local LiveKit server in development mode, and enable the E2E test feature:

```sh
livekit-server --dev
cargo test --features default,__lk-e2e-test -- --nocapture
```

## Peer Connection Signaling Tests

The `peer_connection_signaling_test.rs` tests verify both V0 (dual peer connection) and V1 (single peer connection) signaling modes.

### V0 Tests (Dual PC - works on localhost)

V0 tests can run against a local LiveKit server:

```sh
# Start local server
livekit-server --dev

# Run V0 tests only (uses localhost by default)
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v0_ -- --nocapture
```

### V1 Tests (Single PC - requires LiveKit Cloud)

V1 (single peer connection) mode requires a LiveKit Cloud server or a server that supports the `/rtc/v1` endpoint. Local development servers typically don't support V1 signaling.

Set the following environment variables:

```sh
export LIVEKIT_URL="wss://your-project.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
```

Then run:

```sh
# Run V1 tests only
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v1_ -- --nocapture

# Run all signaling tests (both V0 and V1)
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test -- --nocapture
```

**Note:** V1 tests will be skipped if the environment variables are not set.

## VS Code Integration

If you are using Rust Analyzer in Visual Studio Code, you can enable the E2E test feature to get code completion for these tests. Add the following setting to *.vscode/settings.json*:

```json
"rust-analyzer.cargo.features": ["default", "__lk-e2e-test"]
```
