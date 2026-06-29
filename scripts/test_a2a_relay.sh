#!/usr/bin/env bash
# scripts/test_a2a_relay.sh
#
# End-to-end test for livekit-a2a-relay inside rust-sdks.
#
# What this script does:
#   1. Builds the mock A2A agent and the relay example.
#   2. Starts livekit-server in dev mode (localhost:7880).
#   3. Starts the mock A2A agent (localhost:8765).
#   4. Runs the relay example pointing at both.
#   5. Waits 20 seconds to observe audio routing.
#   6. Verifies log output, then tears everything down cleanly.
#
# Usage:
#   ./scripts/test_a2a_relay.sh
#
# Requirements:
#   - livekit-server in PATH (already installed at /usr/local/bin/livekit-server)
#   - cargo in PATH
#   - Ports 7880 (LiveKit) and 8765 (mock agent) must be free
#
# Exit codes:
#   0 — all processes started and logs look healthy
#   1 — build or startup failure

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_DIR="$REPO_ROOT/target/e2e-logs"
mkdir -p "$LOG_DIR"

LK_URL="ws://localhost:7880"
LK_API_KEY="devkey"
LK_API_SECRET="devsecret"
AGENT_URL="http://localhost:8765"
ROOM_NAME="a2a-e2e-test"

# Cleanup on exit
PIDS=()
cleanup() {
    echo ""
    echo "==> Stopping all test processes..."
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait "${PIDS[@]}" 2>/dev/null || true
    echo "==> Done. Logs saved to $LOG_DIR"
}
trap cleanup EXIT INT TERM

# ── 1. Build ────────────────────────────────────────────────────────────────
echo "==> Building (this may take a minute on first run)..."
cd "$REPO_ROOT"
cargo build -p a2a_mock_agent -p a2a_relay_example 2>&1 | tail -5
echo "==> Build OK"

# ── 2. Start LiveKit server ──────────────────────────────────────────────────
echo "==> Starting livekit-server on :7880 (dev mode)..."
livekit-server \
    --dev \
    --keys "${LK_API_KEY}: ${LK_API_SECRET}" \
    --bind 127.0.0.1 \
    > "$LOG_DIR/livekit-server.log" 2>&1 &
PIDS+=($!)
LK_PID=$!

# Wait for LiveKit to be ready
for i in $(seq 1 20); do
    if curl -sf "http://localhost:7880" > /dev/null 2>&1; then
        echo "==> LiveKit server ready (attempt $i)"
        break
    fi
    sleep 0.5
done

# ── 3. Start mock A2A agent ──────────────────────────────────────────────────
echo "==> Starting mock A2A agent on :8765..."
"$REPO_ROOT/target/debug/a2a_mock_agent" \
    > "$LOG_DIR/mock-agent.log" 2>&1 &
PIDS+=($!)

# Wait for agent card endpoint
for i in $(seq 1 20); do
    if curl -sf "http://localhost:8765/.well-known/agent.json" > /dev/null 2>&1; then
        echo "==> Mock A2A agent ready (attempt $i)"
        break
    fi
    sleep 0.5
done

# ── 4. Start relay example ───────────────────────────────────────────────────
echo "==> Starting a2a_relay_example..."
LIVEKIT_URL="$LK_URL" \
LIVEKIT_API_KEY="$LK_API_KEY" \
LIVEKIT_API_SECRET="$LK_API_SECRET" \
RUST_LOG=info \
"$REPO_ROOT/target/debug/a2a_relay_example" \
    --agent-url "$AGENT_URL" \
    --room-name "$ROOM_NAME" \
    > "$LOG_DIR/relay-example.log" 2>&1 &
PIDS+=($!)
RELAY_PID=$!

echo "==> Relay example PID: $RELAY_PID"
echo "==> Waiting 20 seconds for audio pipeline to stabilise..."
sleep 20

# ── 5. Check logs ────────────────────────────────────────────────────────────
echo ""
echo "=== relay-example.log ==="
cat "$LOG_DIR/relay-example.log"

echo ""
echo "=== mock-agent.log (last 20 lines) ==="
tail -20 "$LOG_DIR/mock-agent.log"

echo ""
echo "==> Checking for expected log markers..."
PASS=true

if grep -q "Connected to room" "$LOG_DIR/relay-example.log"; then
    echo "  ✓ Relay connected to LiveKit room"
else
    echo "  ✗ Relay did NOT connect to LiveKit room"
    PASS=false
fi

if grep -q "Connecting to A2A agent" "$LOG_DIR/relay-example.log"; then
    echo "  ✓ Relay attempted A2A agent connection"
else
    echo "  ✗ Relay did NOT attempt A2A agent connection"
    PASS=false
fi

if grep -q "stream_message" "$LOG_DIR/mock-agent.log" || \
   grep -q "OfficialA2aClient: starting turn" "$LOG_DIR/relay-example.log"; then
    echo "  ✓ At least one A2A streaming turn initiated"
else
    echo "  ~ No A2A turns detected (relay may be waiting for subscriber audio — expected with no remote participant)"
fi

echo ""
if [ "$PASS" = "true" ]; then
    echo "==> ✓ E2E test PASSED"
    exit 0
else
    echo "==> ✗ E2E test FAILED — check $LOG_DIR for details"
    exit 1
fi
