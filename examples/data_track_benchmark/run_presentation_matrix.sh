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

LOSSY_SIZES="${LOSSY_SIZES:-1,4,8,16,32,64}"
LOSSY_FREQUENCIES="${LOSSY_FREQUENCIES:-5,10,100,1000,5000}"
RELIABLE_SIZES="${RELIABLE_SIZES:-16,32,64,128,256,512}"
RELIABLE_FREQUENCIES="${RELIABLE_FREQUENCIES:-10,10,100,500}"
DURATION="${DURATION:-3}"
LOSSY_DRAIN_MS="${LOSSY_DRAIN_MS:-500}"
RELIABLE_DRAIN_MS="${RELIABLE_DRAIN_MS:-10000}"
ROOM="${ROOM:-data-track-benchmark-presentation}"
OUTPUT_DIR="${OUTPUT_DIR:-$WORKSPACE_ROOT/data-track-benchmark-report}"
SERVER_LOG="${SERVER_LOG:-$OUTPUT_DIR/livekit-server.log}"
START_LIVEKIT_SERVER="${START_LIVEKIT_SERVER:-1}"
MAX_EXPECTED_MIBPS="${MAX_EXPECTED_MIBPS:-}"
BUCKET_MS="${BUCKET_MS:-1000}"
EXTRA_ARGS="${EXTRA_ARGS:-}"

LOSSY_RESULTS="$OUTPUT_DIR/lossy-results.csv"
LOSSY_LATENCY="$OUTPUT_DIR/lossy-latency.csv"
LOSSY_PUBLISH="$OUTPUT_DIR/lossy-publish.csv"
RELIABLE_RESULTS="$OUTPUT_DIR/reliable-results.csv"
RELIABLE_LATENCY="$OUTPUT_DIR/reliable-latency.csv"
RELIABLE_PUBLISH="$OUTPUT_DIR/reliable-publish.csv"
RESULTS_CSV="$OUTPUT_DIR/results.csv"
LATENCY_CSV="$OUTPUT_DIR/latency.csv"
PUBLISH_CSV="$OUTPUT_DIR/publish.csv"

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

append_csv() {
  local src="$1"
  local dst="$2"

  if [[ ! -s "$src" ]]; then
    echo "missing CSV: $src" >&2
    exit 1
  fi
  if [[ ! -s "$dst" ]]; then
    cp "$src" "$dst"
  else
    tail -n +2 "$src" >> "$dst"
  fi
}

run_benchmark() {
  local reliability="$1"
  local sizes="$2"
  local frequencies="$3"
  local drain_ms="$4"
  local results="$5"
  local latency="$6"
  local publish="$7"

  (
    cd "$RUST_SDKS_ROOT"
    benchmark_args=(
      --url "$LIVEKIT_URL"
      --api-key "$LIVEKIT_API_KEY"
      --api-secret "$LIVEKIT_API_SECRET"
      --room "$ROOM-$reliability"
      --sizes "$sizes"
      --frequencies "$frequencies"
      --duration "$DURATION"
      --reliability "$reliability"
      --drain-ms "$drain_ms"
      --output "$results"
      --latency-output "$latency"
      --publish-output "$publish"
    )
    if [[ -n "$MAX_EXPECTED_MIBPS" ]]; then
      benchmark_args+=(--max-expected-mibps "$MAX_EXPECTED_MIBPS")
    fi
    if [[ -n "$EXTRA_ARGS" ]]; then
      # shellcheck disable=SC2206
      benchmark_args+=($EXTRA_ARGS)
    fi
    cargo run -p data_track_benchmark -- "${benchmark_args[@]}"
  )
}

mkdir -p "$OUTPUT_DIR"
rm -f "$RESULTS_CSV" "$LATENCY_CSV" "$PUBLISH_CSV" \
  "$LOSSY_RESULTS" "$LOSSY_LATENCY" "$LOSSY_PUBLISH" \
  "$RELIABLE_RESULTS" "$RELIABLE_LATENCY" "$RELIABLE_PUBLISH"

if [[ "$START_LIVEKIT_SERVER" == "1" ]]; then
  if [[ ! -x "$LIVEKIT_SERVER_BIN" ]]; then
    echo "livekit-server binary not found or not executable: $LIVEKIT_SERVER_BIN" >&2
    echo "Build it with: (cd $WORKSPACE_ROOT/livekit && mage)" >&2
    exit 1
  fi

  LIVEKIT_DATA_TRACK_STATS_INTERVAL_MS=1000 \
    "$LIVEKIT_SERVER_BIN" --dev --node-ip "$LIVEKIT_HOST" >"$SERVER_LOG" 2>&1 &
  server_pid="$!"

  if ! wait_for_port "$LIVEKIT_HOST" "$LIVEKIT_PORT"; then
    echo "livekit-server did not open $LIVEKIT_HOST:$LIVEKIT_PORT" >&2
    echo "Server log: $SERVER_LOG" >&2
    exit 1
  fi
else
  : > "$SERVER_LOG"
fi

echo "lossy matrix: sizes=$LOSSY_SIZES frequencies=$LOSSY_FREQUENCIES duration=${DURATION}s"
run_benchmark lossy "$LOSSY_SIZES" "$LOSSY_FREQUENCIES" "$LOSSY_DRAIN_MS" \
  "$LOSSY_RESULTS" "$LOSSY_LATENCY" "$LOSSY_PUBLISH"

echo "reliable matrix: sizes=$RELIABLE_SIZES frequencies=$RELIABLE_FREQUENCIES duration=${DURATION}s"
run_benchmark reliable "$RELIABLE_SIZES" "$RELIABLE_FREQUENCIES" "$RELIABLE_DRAIN_MS" \
  "$RELIABLE_RESULTS" "$RELIABLE_LATENCY" "$RELIABLE_PUBLISH"

append_csv "$LOSSY_RESULTS" "$RESULTS_CSV"
append_csv "$RELIABLE_RESULTS" "$RESULTS_CSV"
append_csv "$LOSSY_LATENCY" "$LATENCY_CSV"
append_csv "$RELIABLE_LATENCY" "$LATENCY_CSV"
append_csv "$LOSSY_PUBLISH" "$PUBLISH_CSV"
append_csv "$RELIABLE_PUBLISH" "$PUBLISH_CSV"

python3 "$SCRIPT_DIR/report.py" "$OUTPUT_DIR" --bucket-ms "$BUCKET_MS"

echo "data track presentation benchmark passed"
echo "Report PDF: $OUTPUT_DIR/report.pdf"
echo "Report HTML: $OUTPUT_DIR/report.html"
echo "Summary CSV: $RESULTS_CSV"
echo "Latency CSV: $LATENCY_CSV"
echo "Publish CSV: $PUBLISH_CSV"
echo "Time series CSV: $OUTPUT_DIR/timeseries.csv"
echo "SFU time series CSV: $OUTPUT_DIR/sfu_timeseries.csv"
echo "Server log: $SERVER_LOG"
