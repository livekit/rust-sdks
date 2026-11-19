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

### V0 Tests (Dual Peer Connection)

V0 tests work on localhost with the default LiveKit development server:

```sh
# Start local server
livekit-server --dev

# Run V0 tests
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v0_ -- --nocapture
```

Default localhost configuration:
- URL: `ws://localhost:7880`
- API Key: `devkey`
- API Secret: `secret`

### V1 Tests (Single Peer Connection)

**Important:** V1 (single peer connection) mode requires a LiveKit Cloud server or a server that supports the `/rtc/v1` endpoint.

⚠️ **V1 tests will fall back to V0 signaling on localhost**, so to truly test V1 functionality, you **must** set the cloud environment variables:

```sh
export LIVEKIT_URL="wss://your-project.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"

# Run V1 tests
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test v1_ -- --nocapture
```

### Running All Tests

```sh
# On localhost (V0 will work, V1 falls back to V0)
# Note, suggest running with --test-threads=1 to avoid flakiness
livekit-server --dev
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test -- --nocapture --test-threads=1

# On LiveKit Cloud (both V0 and V1 work correctly)
export LIVEKIT_URL="wss://your-project.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
cargo test -p livekit --features "__lk-e2e-test,native-tls" --test peer_connection_signaling_test -- --nocapture --test-threads=1
```

## VS Code Integration

If you are using Rust Analyzer in Visual Studio Code, you can enable the E2E test feature to get code completion for these tests. Add the following setting to *.vscode/settings.json*:

```json
"rust-analyzer.cargo.features": ["default", "__lk-e2e-test"]
```

## Fast track publication

`fast_publish_test.rs` tests ordinary publication with server `fast_publish` enabled and disabled, in single- and dual-peer-connection mode, both with and without `RoomOptions::publish_tracks` at connect time. Its bounded WebSocket proxy holds publication acknowledgments independently of SDP answers to prove the ordering without timing assertions. It also covers concurrent muted audio/video publication, codec/encryption/packet-trailer fallbacks, rejection, cancellation cleanup, duplicate publication, and full reconnect.

Use a recent server that supports `/rtc/v1`, with isolated ports and loopback candidates:

```yaml
# /tmp/fast-publish-server.yaml
port: 17880
rtc:
  tcp_port: 17881
  udp_port: 17882
  use_external_ip: false
  enable_loopback_candidate: true
keys:
  devkey: secret
```

```sh
livekit-server --dev --node-ip 127.0.0.1 --config /tmp/fast-publish-server.yaml
LIVEKIT_TEST_SERVER_URL=ws://127.0.0.1:17880 \
  cargo test -p livekit --features '__lk-e2e-test,native-tls' \
  --test fast_publish_test -- --nocapture --test-threads=1
```

The `publication_latency_benchmark` test measures publish-call completion and the first video frame actually received by another participant. It uses dual peer connections to measure an already-warmed publisher independently of subscriber negotiation. It adds 50 ms in each signaling direction (100 ms signaling RTT), leaving media transport unchanged. Both arms keep eager publisher negotiation enabled; clearing the advertised codecs forces the sequential fallback in the baseline arm. This isolates registration/SDP overlap from publisher warm-up and negotiation debounce. It also records the time from sending AddTrack to sending the track’s offer. A data-channel send before each timed publication verifies the publisher transport is ready. The test alternates arm order and prints raw samples plus first-frame medians. Set `FAST_PUBLISH_BENCH_ITERATIONS` to change the default five samples per arm. It intentionally has no latency threshold: scheduling, encoder startup, and keyframes affect first-frame timing.

`LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET` override the default development credentials. Tests create fresh room names and explicitly close their rooms and proxy tasks.
