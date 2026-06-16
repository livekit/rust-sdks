#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_SDKS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKSPACE_ROOT="$(cd "$RUST_SDKS_ROOT/.." && pwd)"

LIVEKIT_HOST="${LIVEKIT_HOST:-127.0.0.1}"
LIVEKIT_PORT="${LIVEKIT_PORT:-7880}"
LIVEKIT_URL="${LIVEKIT_URL:-ws://$LIVEKIT_HOST:$LIVEKIT_PORT}"
LIVEKIT_API_KEY="${LIVEKIT_API_KEY:-devkey}"
LIVEKIT_API_SECRET="${LIVEKIT_API_SECRET:-secret}"
LIVEKIT_SERVER_BIN="${LIVEKIT_SERVER_BIN:-$WORKSPACE_ROOT/livekit/bin/livekit-server}"

SIZES="${SIZES:-1,16,64}"
FREQUENCIES="${FREQUENCIES:-1,10,50}"
DURATION="${DURATION:-3}"
RELIABILITY="${RELIABILITY:-both}"
EXTRA_ARGS="${EXTRA_ARGS:-}"
ROOM="${ROOM:-data-track-benchmark-harness}"
OUTPUT_DIR="${OUTPUT_DIR:-$(mktemp -d -t livekit-data-track-bench.XXXXXX)}"
OUTPUT_CSV="${OUTPUT_CSV:-$OUTPUT_DIR/results.csv}"
LATENCY_CSV="${LATENCY_CSV:-$OUTPUT_DIR/latency.csv}"
PUBLISH_CSV="${PUBLISH_CSV:-$OUTPUT_DIR/publish.csv}"
SERVER_LOG="${SERVER_LOG:-$OUTPUT_DIR/livekit-server.log}"
START_LIVEKIT_SERVER="${START_LIVEKIT_SERVER:-1}"
MAX_EXPECTED_MIBPS="${MAX_EXPECTED_MIBPS:-}"
DRAIN_MS="${DRAIN_MS:-}"
RELIABLE_DRAIN_MS="${RELIABLE_DRAIN_MS:-10000}"

server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

wait_for_port() {
  local host="$1"
  local port="$2"

  for _ in $(seq 1 100); do
    if nc -z "$host" "$port" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done

  return 1
}

mkdir -p "$OUTPUT_DIR"

if [[ "$START_LIVEKIT_SERVER" == "1" ]]; then
  if [[ ! -x "$LIVEKIT_SERVER_BIN" ]]; then
    echo "livekit-server binary not found or not executable: $LIVEKIT_SERVER_BIN" >&2
    echo "Build it with: (cd $WORKSPACE_ROOT/livekit && mage)" >&2
    exit 1
  fi

  "$LIVEKIT_SERVER_BIN" --dev --node-ip "$LIVEKIT_HOST" >"$SERVER_LOG" 2>&1 &
  server_pid="$!"

  if ! wait_for_port "$LIVEKIT_HOST" "$LIVEKIT_PORT"; then
    echo "livekit-server did not open $LIVEKIT_HOST:$LIVEKIT_PORT" >&2
    echo "Server log: $SERVER_LOG" >&2
    exit 1
  fi
fi

(
  cd "$RUST_SDKS_ROOT"
  benchmark_args=(
    --url "$LIVEKIT_URL" \
    --api-key "$LIVEKIT_API_KEY" \
    --api-secret "$LIVEKIT_API_SECRET" \
    --room "$ROOM" \
    --sizes "$SIZES" \
    --frequencies "$FREQUENCIES" \
    --duration "$DURATION" \
    --reliability "$RELIABILITY" \
    --output "$OUTPUT_CSV" \
    --latency-output "$LATENCY_CSV" \
    --publish-output "$PUBLISH_CSV"
  )
  if [[ -n "$MAX_EXPECTED_MIBPS" ]]; then
    benchmark_args+=(--max-expected-mibps "$MAX_EXPECTED_MIBPS")
  fi
  if [[ -n "$DRAIN_MS" ]]; then
    benchmark_args+=(--drain-ms "$DRAIN_MS")
  fi
  if [[ -n "$RELIABLE_DRAIN_MS" ]]; then
    benchmark_args+=(--reliable-drain-ms "$RELIABLE_DRAIN_MS")
  fi
  if [[ -n "$EXTRA_ARGS" ]]; then
    # shellcheck disable=SC2206
    benchmark_args+=($EXTRA_ARGS)
  fi
  cargo run -p data_track_benchmark -- "${benchmark_args[@]}"
)

lossy_rows="$(grep -c '^lossy,' "$OUTPUT_CSV" || true)"
reliable_rows="$(grep -c '^reliable,' "$OUTPUT_CSV" || true)"

if [[ "$RELIABILITY" == "both" && ("$lossy_rows" == "0" || "$reliable_rows" == "0") ]]; then
  echo "benchmark did not produce both reliability modes" >&2
  echo "lossy rows: $lossy_rows" >&2
  echo "reliable rows: $reliable_rows" >&2
  echo "CSV: $OUTPUT_CSV" >&2
  exit 1
fi
if [[ "$RELIABILITY" == "lossy" && "$lossy_rows" == "0" ]]; then
  echo "benchmark did not produce lossy rows" >&2
  echo "CSV: $OUTPUT_CSV" >&2
  exit 1
fi
if [[ "$RELIABILITY" == "reliable" && "$reliable_rows" == "0" ]]; then
  echo "benchmark did not produce reliable rows" >&2
  echo "CSV: $OUTPUT_CSV" >&2
  exit 1
fi

echo "data track benchmark harness passed"
echo "CSV: $OUTPUT_CSV"
echo "Latency CSV: $LATENCY_CSV"
echo "Publish CSV: $PUBLISH_CSV"
if [[ "$START_LIVEKIT_SERVER" == "1" ]]; then
  echo "Server log: $SERVER_LOG"
fi
