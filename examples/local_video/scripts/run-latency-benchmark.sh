#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/../../.." && pwd)
RESULT_ROOT="$REPO_ROOT/target/local_video_latency"
LOG_FILTER="${LOCAL_VIDEO_BENCH_RUST_LOG:-info}"

NAME=""
DURATION=60
WARMUP=8
URL="${LIVEKIT_URL:-}"
API_KEY="${LIVEKIT_API_KEY:-}"
API_SECRET="${LIVEKIT_API_SECRET:-}"
WIDTH=1280
HEIGHT=720
FPS=30
CODEC=h264
ENCODER=""
DECODER=default
RENDER_PATH=""
HEADLESS=0
RENDER_VSYNC=""
KEEP_WINDOW_FRONT=0
RENDER_LOOP_DIAGNOSTICS=0
DROP_LATE_FRAMES_MS=0
CAMERA_INDEX=0
SOURCE=""
FORMAT=""
TEST_PATTERN=0
DEGRADATION_PREFERENCE=maintain-resolution
MIN_PLAYOUT_DELAY=0
MAX_PLAYOUT_DELAY=1
PUBLISHER_BIN="$REPO_ROOT/target/release/publisher"
SUBSCRIBER_BIN="$REPO_ROOT/target/release/subscriber"
PUBLISHER_IDENTITY=""
SUBSCRIBER_IDENTITY=""
NO_OVERLAY=1
NO_STATS=1
OVERWRITE=0
FAIL_ON_STUTTER=0
REQUIRE_BENCHMARK_PASS=0
MIN_FRAME_COVERAGE_PCT=95
MIN_TIME_COVERAGE_PCT=95
CAFFEINATE_MODE=auto
HOST_LOAD_INTERVAL=5
WAIT_FOR_IDLE_HOST=0
IDLE_CONFIRMATION_SAMPLES=3
HOST_BUSY_PROCESS_CPU_PCT=50
HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT=150
MAX_SINK_GAP_P95_MS=""
MAX_PAINT_GAP_P95_MS=""
MAX_E2E_P95_MS=""
MAX_RECEIVE_TO_DECODE_P95_MS=""
MAX_RECEIVE_TO_PAINT_P95_MS=""
MAX_CAPTURE_TO_PACKETIZE_P95_MS=""
MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS=""
PUBLISHER_EXTRA=()

usage() {
  cat <<'USAGE'
Usage:
  run-latency-benchmark.sh --name NAME --url URL --api-key KEY --api-secret SECRET [options]

Runs local_video publisher and subscriber for a fixed duration, logs both sides, then
generates latency-timeseries.csv, stutters.csv, receiver-stats.csv,
worst-windows.csv, summary.json, report.md, and report.pdf.

The benchmark directory and default room name are both:
  target/local_video_latency/NAME

Required:
  --name NAME
  --url URL                    or LIVEKIT_URL
  --api-key KEY                or LIVEKIT_API_KEY
  --api-secret SECRET          or LIVEKIT_API_SECRET

Publisher setup:
  --width PX                   default: 1280
  --height PX                  default: 720
  --fps FPS                    default: 30
  --codec CODEC                default: h264
  --encoder ENCODER            example: software, nvenc
  --camera-index INDEX         default: 0
  --source SOURCE              example: uvc, argus
  --format FORMAT              example: auto, yuv, mjpeg
  --test-pattern               generate color bars instead of opening a camera
  --degradation-preference V   default: maintain-resolution; best-effort sender hint
  --min-playout-delay MS       publisher-side room setting; default: 0
  --max-playout-delay MS       publisher-side room setting; default: 1
  --publisher-arg ARG          append one raw publisher argument; repeat for values

Subscriber setup:
  --decoder DECODER            default: default; use software to bypass known hardware decoders
  --render-path PATH           default: cpu on macOS visible runs, auto otherwise
  --headless                   consume decoded frames without opening a subscriber window
  --render-vsync               use vsync presentation for the subscriber window
  --no-render-vsync            use immediate/no-vsync presentation for the subscriber window
                               default: no-vsync on macOS visible runs, vsync otherwise
  --keep-window-front          focus and keep the subscriber window above other windows
                               default: backgroundable visible window
  --background-window          do not focus or keep the subscriber window above other windows
  --render-loop-diagnostics    log subscriber render-loop scheduling diagnostics
  --drop-late-frames-ms MS     drop decoded frames older than this before render; default: 0
  --overlay                    show subscriber overlays and controls
  --stats                      enable subscriber getStats polling

Latency budgets:
  --max-sink-gap-p95-ms MS
  --max-paint-gap-p95-ms MS
  --max-e2e-p95-ms MS
  --max-receive-to-decode-p95-ms MS
  --max-receive-to-paint-p95-ms MS
  --max-capture-to-packetize-p95-ms MS
  --max-encoder-upload-to-output-p95-ms MS
                               optional p95 window-max thresholds; require-benchmark-pass fails
                               if a clean run exceeds or cannot compute a configured metric

Run control:
  --duration SECONDS           default: 60
  --warmup SECONDS             startup time to exclude before measured duration; default: 8
  --publisher-bin PATH         default: target/release/publisher
  --subscriber-bin PATH        default: target/release/subscriber
  --publisher-identity ID      default: NAME-publisher
  --subscriber-identity ID     default: NAME-subscriber
  --overwrite                  replace an existing target/local_video_latency/NAME
  --fail-on-stutter            exit non-zero if any render stutter or visible frame skip is logged
  --require-benchmark-pass     exit non-zero unless the run is valid, smooth, complete, and host-clean
  --min-frame-coverage-pct PCT default: 95; minimum frame coverage for benchmark pass
  --min-time-coverage-pct PCT  default: 95; minimum observed time coverage for benchmark pass
  --caffeinate                 prevent macOS idle sleep/App Nap during the run
                               default: auto on macOS when caffeinate exists
  --no-caffeinate              disable the macOS caffeinate guard
  --host-load-interval SECONDS sample host load while the run is active; default: 5, 0 disables
  --wait-for-idle-host SECONDS wait up to this long before starting if host load is busy
  --idle-confirmation-samples N
                               default: 3; consecutive idle samples required before starting
  --host-busy-process-cpu-pct PCT
                               default: 50; process CPU threshold for busy host status/preflight
  --host-busy-total-cpu-pct PCT
                               default: 150; external total CPU threshold for busy host status/preflight
USAGE
}

require_value() {
  local flag=$1
  local value=${2:-}
  if [[ -z "$value" ]]; then
    echo "error: $flag requires a value" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name) require_value "$1" "${2:-}"; NAME=$2; shift 2 ;;
    --duration) require_value "$1" "${2:-}"; DURATION=$2; shift 2 ;;
    --warmup) require_value "$1" "${2:-}"; WARMUP=$2; shift 2 ;;
    --url) require_value "$1" "${2:-}"; URL=$2; shift 2 ;;
    --api-key) require_value "$1" "${2:-}"; API_KEY=$2; shift 2 ;;
    --api-secret) require_value "$1" "${2:-}"; API_SECRET=$2; shift 2 ;;
    --width) require_value "$1" "${2:-}"; WIDTH=$2; shift 2 ;;
    --height) require_value "$1" "${2:-}"; HEIGHT=$2; shift 2 ;;
    --fps) require_value "$1" "${2:-}"; FPS=$2; shift 2 ;;
    --codec) require_value "$1" "${2:-}"; CODEC=$2; shift 2 ;;
    --encoder) require_value "$1" "${2:-}"; ENCODER=$2; shift 2 ;;
    --decoder) require_value "$1" "${2:-}"; DECODER=$2; shift 2 ;;
    --render-path) require_value "$1" "${2:-}"; RENDER_PATH=$2; shift 2 ;;
    --headless) HEADLESS=1; shift ;;
    --render-vsync) RENDER_VSYNC=1; shift ;;
    --no-render-vsync) RENDER_VSYNC=0; shift ;;
    --keep-window-front) KEEP_WINDOW_FRONT=1; shift ;;
    --background-window) KEEP_WINDOW_FRONT=0; shift ;;
    --render-loop-diagnostics) RENDER_LOOP_DIAGNOSTICS=1; shift ;;
    --drop-late-frames-ms) require_value "$1" "${2:-}"; DROP_LATE_FRAMES_MS=$2; shift 2 ;;
    --camera-index) require_value "$1" "${2:-}"; CAMERA_INDEX=$2; shift 2 ;;
    --source) require_value "$1" "${2:-}"; SOURCE=$2; shift 2 ;;
    --format) require_value "$1" "${2:-}"; FORMAT=$2; shift 2 ;;
    --test-pattern) TEST_PATTERN=1; shift ;;
    --degradation-preference) require_value "$1" "${2:-}"; DEGRADATION_PREFERENCE=$2; shift 2 ;;
    --min-playout-delay) require_value "$1" "${2:-}"; MIN_PLAYOUT_DELAY=$2; shift 2 ;;
    --max-playout-delay) require_value "$1" "${2:-}"; MAX_PLAYOUT_DELAY=$2; shift 2 ;;
    --publisher-bin) require_value "$1" "${2:-}"; PUBLISHER_BIN=$2; shift 2 ;;
    --subscriber-bin) require_value "$1" "${2:-}"; SUBSCRIBER_BIN=$2; shift 2 ;;
    --publisher-identity) require_value "$1" "${2:-}"; PUBLISHER_IDENTITY=$2; shift 2 ;;
    --subscriber-identity) require_value "$1" "${2:-}"; SUBSCRIBER_IDENTITY=$2; shift 2 ;;
    --publisher-arg) require_value "$1" "${2:-}"; PUBLISHER_EXTRA+=("$2"); shift 2 ;;
    --overlay) NO_OVERLAY=0; shift ;;
    --stats) NO_STATS=0; shift ;;
    --overwrite) OVERWRITE=1; shift ;;
    --fail-on-stutter) FAIL_ON_STUTTER=1; shift ;;
    --require-benchmark-pass) REQUIRE_BENCHMARK_PASS=1; shift ;;
    --min-frame-coverage-pct) require_value "$1" "${2:-}"; MIN_FRAME_COVERAGE_PCT=$2; shift 2 ;;
    --min-time-coverage-pct) require_value "$1" "${2:-}"; MIN_TIME_COVERAGE_PCT=$2; shift 2 ;;
    --caffeinate) CAFFEINATE_MODE=1; shift ;;
    --no-caffeinate) CAFFEINATE_MODE=0; shift ;;
    --host-load-interval) require_value "$1" "${2:-}"; HOST_LOAD_INTERVAL=$2; shift 2 ;;
    --wait-for-idle-host) require_value "$1" "${2:-}"; WAIT_FOR_IDLE_HOST=$2; shift 2 ;;
    --idle-confirmation-samples) require_value "$1" "${2:-}"; IDLE_CONFIRMATION_SAMPLES=$2; shift 2 ;;
    --host-busy-process-cpu-pct) require_value "$1" "${2:-}"; HOST_BUSY_PROCESS_CPU_PCT=$2; shift 2 ;;
    --host-busy-total-cpu-pct) require_value "$1" "${2:-}"; HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT=$2; shift 2 ;;
    --max-sink-gap-p95-ms) require_value "$1" "${2:-}"; MAX_SINK_GAP_P95_MS=$2; shift 2 ;;
    --max-paint-gap-p95-ms) require_value "$1" "${2:-}"; MAX_PAINT_GAP_P95_MS=$2; shift 2 ;;
    --max-e2e-p95-ms) require_value "$1" "${2:-}"; MAX_E2E_P95_MS=$2; shift 2 ;;
    --max-receive-to-decode-p95-ms) require_value "$1" "${2:-}"; MAX_RECEIVE_TO_DECODE_P95_MS=$2; shift 2 ;;
    --max-receive-to-paint-p95-ms) require_value "$1" "${2:-}"; MAX_RECEIVE_TO_PAINT_P95_MS=$2; shift 2 ;;
    --max-capture-to-packetize-p95-ms) require_value "$1" "${2:-}"; MAX_CAPTURE_TO_PACKETIZE_P95_MS=$2; shift 2 ;;
    --max-encoder-upload-to-output-p95-ms) require_value "$1" "${2:-}"; MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; usage >&2; exit 1 ;;
  esac
done

if [[ -z "$NAME" ]]; then
  echo "error: --name is required" >&2
  exit 1
fi
if [[ "$NAME" == *"/"* || "$NAME" == "." || "$NAME" == ".." ]]; then
  echo "error: --name must be a single directory name, not a path" >&2
  exit 1
fi
if [[ -z "$URL" || -z "$API_KEY" || -z "$API_SECRET" ]]; then
  echo "error: --url, --api-key, and --api-secret are required unless set via environment" >&2
  exit 1
fi
if [[ ! "$HOST_LOAD_INTERVAL" =~ ^[0-9]+$ ]]; then
  echo "error: --host-load-interval must be a non-negative integer" >&2
  exit 1
fi
if [[ ! "$WAIT_FOR_IDLE_HOST" =~ ^[0-9]+$ ]]; then
  echo "error: --wait-for-idle-host must be a non-negative integer" >&2
  exit 1
fi
if [[ ! "$IDLE_CONFIRMATION_SAMPLES" =~ ^[0-9]+$ || "$IDLE_CONFIRMATION_SAMPLES" -lt 1 ]]; then
  echo "error: --idle-confirmation-samples must be a positive integer" >&2
  exit 1
fi
if [[ ! "$HOST_BUSY_PROCESS_CPU_PCT" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "error: --host-busy-process-cpu-pct must be a non-negative number" >&2
  exit 1
fi
if [[ ! "$HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "error: --host-busy-total-cpu-pct must be a non-negative number" >&2
  exit 1
fi
if [[ ! "$DROP_LATE_FRAMES_MS" =~ ^[0-9]+$ ]]; then
  echo "error: --drop-late-frames-ms must be a non-negative integer" >&2
  exit 1
fi
for coverage in \
  "min-frame-coverage-pct:$MIN_FRAME_COVERAGE_PCT" \
  "min-time-coverage-pct:$MIN_TIME_COVERAGE_PCT"
do
  flag=${coverage%%:*}
  value=${coverage#*:}
  if [[ ! "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "error: --$flag must be a non-negative number" >&2
    exit 1
  fi
  if ! awk "BEGIN { exit !($value >= 0 && $value <= 100) }"; then
    echo "error: --$flag must be between 0 and 100" >&2
    exit 1
  fi
done
for budget in \
  "max-sink-gap-p95-ms:$MAX_SINK_GAP_P95_MS" \
  "max-paint-gap-p95-ms:$MAX_PAINT_GAP_P95_MS" \
  "max-e2e-p95-ms:$MAX_E2E_P95_MS" \
  "max-receive-to-decode-p95-ms:$MAX_RECEIVE_TO_DECODE_P95_MS" \
  "max-receive-to-paint-p95-ms:$MAX_RECEIVE_TO_PAINT_P95_MS" \
  "max-capture-to-packetize-p95-ms:$MAX_CAPTURE_TO_PACKETIZE_P95_MS" \
  "max-encoder-upload-to-output-p95-ms:$MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS"
do
  flag=${budget%%:*}
  value=${budget#*:}
  if [[ -n "$value" && ! "$value" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "error: --$flag must be a non-negative number" >&2
    exit 1
  fi
done
case "$DECODER" in
  default|platform|software) ;;
  *)
    echo "error: --decoder must be default, platform, or software" >&2
    exit 1
    ;;
esac
if [[ -z "$RENDER_PATH" ]]; then
  if [[ "$(uname -s)" == "Darwin" && "$HEADLESS" -eq 0 ]]; then
    RENDER_PATH=cpu
  else
    RENDER_PATH=auto
  fi
else
  case "$RENDER_PATH" in
    auto|cpu) ;;
    *)
      echo "error: --render-path must be auto or cpu" >&2
      exit 1
      ;;
  esac
fi
if [[ -z "$RENDER_VSYNC" ]]; then
  if [[ "$(uname -s)" == "Darwin" && "$HEADLESS" -eq 0 ]]; then
    RENDER_VSYNC=0
  else
    RENDER_VSYNC=1
  fi
fi
CAFFEINATE_ACTIVE=0
if [[ "$CAFFEINATE_MODE" == "1" ]]; then
  if ! command -v caffeinate >/dev/null 2>&1; then
    echo "error: --caffeinate was requested, but caffeinate was not found" >&2
    exit 1
  fi
  CAFFEINATE_ACTIVE=1
elif [[ "$CAFFEINATE_MODE" == "auto" ]]; then
  if [[ "$(uname -s)" == "Darwin" ]] && command -v caffeinate >/dev/null 2>&1; then
    CAFFEINATE_ACTIVE=1
  fi
fi

if [[ ! -x "$PUBLISHER_BIN" || ! -x "$SUBSCRIBER_BIN" ]]; then
  echo "release binaries not found; building publisher and subscriber"
  (cd "$REPO_ROOT" && cargo build -p local_video --features desktop --release --bin publisher --bin subscriber)
fi

PUBLISHER_IDENTITY=${PUBLISHER_IDENTITY:-"$NAME-publisher"}
SUBSCRIBER_IDENTITY=${SUBSCRIBER_IDENTITY:-"$NAME-subscriber"}
RUN_DIR="$RESULT_ROOT/$NAME"
DISABLE_SPOTLIGHT=0

mkdir -p "$RESULT_ROOT"
if [[ "$(uname -s)" == "Darwin" ]]; then
  DISABLE_SPOTLIGHT=1
  touch "$RESULT_ROOT/.metadata_never_index" 2>/dev/null || true
fi

if [[ -e "$RUN_DIR" && "$OVERWRITE" -ne 1 ]]; then
  echo "error: benchmark directory already exists: $RUN_DIR" >&2
  echo "       choose a new --name or pass --overwrite" >&2
  exit 1
fi

host_load_status() {
  if ! command -v ps >/dev/null 2>&1; then
    echo "0 0 none"
    return
  fi

  ps -arcwwwxo pcpu=,comm= 2>/dev/null | awk '
    BEGIN {
      top = 0
      total = 0
      top_name = "none"
    }
    {
      cpu = $1 + 0
      $1 = ""
      sub(/^ +/, "")
      comm = $0
      if (comm == "awk" || comm == "bash" || comm == "caffeinate" ||
          comm == "head" || comm == "livekit-server" ||
          comm == "ps" || comm == "publisher" || comm == "sleep" ||
          comm == "subscriber" || comm == "sysmond" ||
          comm == "VTDecoderXPCService" ||
          comm == "VTEncoderXPCService") {
        next
      }
      total += cpu
      if (cpu > top) {
        top = cpu
        top_name = comm
      }
    }
    END {
      printf "%.1f %.1f %s\n", top, total, top_name
    }
  '
}

wait_for_idle_host() {
  if [[ "$WAIT_FOR_IDLE_HOST" -eq 0 ]]; then
    return
  fi

  local deadline=$((SECONDS + WAIT_FOR_IDLE_HOST))
  local top_cpu total_cpu top_name
  local idle_samples=0
  while true; do
    read -r top_cpu total_cpu top_name < <(host_load_status)
    if awk "BEGIN { exit !($top_cpu < $HOST_BUSY_PROCESS_CPU_PCT && $total_cpu < $HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT) }"; then
      idle_samples=$((idle_samples + 1))
      if [[ "$idle_samples" -ge "$IDLE_CONFIRMATION_SAMPLES" ]]; then
        echo "Host load idle for ${idle_samples}/${IDLE_CONFIRMATION_SAMPLES} samples: top_external=${top_cpu}% total_external=${total_cpu}% (${top_name})"
        return
      fi
      echo "Host load idle sample ${idle_samples}/${IDLE_CONFIRMATION_SAMPLES}: top_external=${top_cpu}% total_external=${total_cpu}% (${top_name})"
    else
      idle_samples=0
      echo "Waiting for idle host: top_external=${top_cpu}% total_external=${total_cpu}% (${top_name})"
    fi
    if [[ "$SECONDS" -ge "$deadline" ]]; then
      echo "error: host stayed busy for ${WAIT_FOR_IDLE_HOST}s; top_external=${top_cpu}% total_external=${total_cpu}% (${top_name})" >&2
      echo "       thresholds: top_external<${HOST_BUSY_PROCESS_CPU_PCT}% total_external<${HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT}%" >&2
      echo "       required idle samples: ${IDLE_CONFIRMATION_SAMPLES}" >&2
      return 4
    fi
    sleep 5
  done
}

if [[ -e "$RUN_DIR" ]]; then
  rm -rf "$RUN_DIR"
fi
mkdir -p "$RUN_DIR"
if [[ "$DISABLE_SPOTLIGHT" -eq 1 ]]; then
  touch "$RUN_DIR/.metadata_never_index" 2>/dev/null || true
fi

capture_host_load() {
  local path=$1
  {
    date -u +"captured_at=%Y-%m-%dT%H:%M:%SZ"
    uname -a
    if command -v ps >/dev/null 2>&1; then
      ps -arcwwwxo pid,pcpu,pmem,comm | head -25
    fi
  } > "$path" 2>/dev/null || true
}

append_host_load_sample() {
  local path=$1
  {
    echo "---"
    date -u +"captured_at=%Y-%m-%dT%H:%M:%SZ"
    if command -v ps >/dev/null 2>&1; then
      ps -arcwwwxo pid,pcpu,pmem,comm | head -25
    fi
  } >> "$path" 2>/dev/null || true
}

write_preflight_failure_artifacts() {
  local top_cpu=$1
  local total_cpu=$2
  local top_name=$3

  python3 - "$RUN_DIR" "$NAME" "$DURATION" "$FPS" "$MIN_FRAME_COVERAGE_PCT" \
    "$MIN_TIME_COVERAGE_PCT" "$HOST_BUSY_PROCESS_CPU_PCT" \
    "$HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT" "$top_cpu" "$total_cpu" "$top_name" <<'PY'
import json
import sys
from datetime import UTC, datetime
from pathlib import Path

run_dir = Path(sys.argv[1])
name = sys.argv[2]
duration = float(sys.argv[3])
fps = float(sys.argv[4])
min_frame_coverage = float(sys.argv[5])
min_time_coverage = float(sys.argv[6])
busy_process_threshold = float(sys.argv[7])
busy_total_threshold = float(sys.argv[8])
top_cpu = float(sys.argv[9])
total_cpu = float(sys.argv[10])
top_name = sys.argv[11]
generated_at = datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")

summary = {
    "name": name,
    "directory": str(run_dir),
    "generated_at": generated_at,
    "valid": False,
    "invalid_reasons": ["host idle preflight failed before starting publisher/subscriber"],
    "benchmark_status": "INCOMPLETE_HOST_BUSY",
    "smoothness_status": "NOT_RUN",
    "coverage": {
        "status": "INCOMPLETE",
        "requested_duration_seconds": duration,
        "requested_fps": fps,
        "minimum_frame_coverage_pct": min_frame_coverage,
        "minimum_time_coverage_pct": min_time_coverage,
    },
    "host_load": {
        "status": "BUSY",
        "snapshots": 1,
        "sample_snapshots": 0,
        "busy_snapshots": 1,
        "max_external_total_cpu_pct": total_cpu,
        "max_external_process_name": top_name,
        "max_external_process_cpu_pct": top_cpu,
        "busy_process_cpu_threshold_pct": busy_process_threshold,
        "busy_external_total_cpu_threshold_pct": busy_total_threshold,
    },
    "latency_budget": {"status": "NOT_RUN", "violations": []},
    "publisher": {},
    "subscriber": {},
}
run_dir.joinpath("summary.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
report = f"""# Local Video Latency Report: {name}

- Benchmark status: INCOMPLETE HOST BUSY
- Generated at: {generated_at}
- Reason: host idle preflight failed before publisher/subscriber startup.
- Last observed external process: {top_name} ({top_cpu:.1f}%)
- Last observed external total CPU: {total_cpu:.1f}%
- Thresholds: top external < {busy_process_threshold:.1f}%, total external < {busy_total_threshold:.1f}%

No latency samples were recorded for this run.
"""
run_dir.joinpath("report.md").write_text(report, encoding="utf-8")
PY
}

publisher_cmd=(
  "$PUBLISHER_BIN"
  --url "$URL"
  --api-key "$API_KEY"
  --api-secret "$API_SECRET"
  --room-name "$NAME"
  --identity "$PUBLISHER_IDENTITY"
  --width "$WIDTH"
  --height "$HEIGHT"
  --fps "$FPS"
  --codec "$CODEC"
  --degradation-preference "$DEGRADATION_PREFERENCE"
  --min-playout-delay "$MIN_PLAYOUT_DELAY"
  --max-playout-delay "$MAX_PLAYOUT_DELAY"
  --attach-timestamp
  --attach-frame-id
)

if [[ -n "$ENCODER" ]]; then
  publisher_cmd+=(--encoder "$ENCODER")
fi
if [[ "$TEST_PATTERN" -eq 1 ]]; then
  publisher_cmd+=(--test-pattern)
else
  publisher_cmd+=(--camera-index "$CAMERA_INDEX")
  if [[ -n "$SOURCE" ]]; then
    publisher_cmd+=(--source "$SOURCE")
  fi
  if [[ -n "$FORMAT" ]]; then
    publisher_cmd+=(--format "$FORMAT")
  fi
fi
if [[ ${#PUBLISHER_EXTRA[@]} -gt 0 ]]; then
  publisher_cmd+=("${PUBLISHER_EXTRA[@]}")
fi

subscriber_cmd=(
  "$SUBSCRIBER_BIN"
  --url "$URL"
  --api-key "$API_KEY"
  --api-secret "$API_SECRET"
  --room-name "$NAME"
  --identity "$SUBSCRIBER_IDENTITY"
  --participant "$PUBLISHER_IDENTITY"
  --render-path "$RENDER_PATH"
)

if [[ "$NO_OVERLAY" -eq 1 ]]; then
  subscriber_cmd+=(--no-overlay)
fi
if [[ "$HEADLESS" -eq 1 ]]; then
  subscriber_cmd+=(--headless)
fi
if [[ "$RENDER_VSYNC" -eq 1 && "$HEADLESS" -eq 0 ]]; then
  subscriber_cmd+=(--render-vsync)
fi
if [[ "$KEEP_WINDOW_FRONT" -eq 1 && "$HEADLESS" -eq 0 ]]; then
  subscriber_cmd+=(--keep-window-front)
fi
if [[ "$RENDER_LOOP_DIAGNOSTICS" -eq 1 && "$HEADLESS" -eq 0 ]]; then
  subscriber_cmd+=(--render-loop-diagnostics)
fi
if [[ "$DROP_LATE_FRAMES_MS" -gt 0 ]]; then
  subscriber_cmd+=(--drop-late-frames-ms "$DROP_LATE_FRAMES_MS")
fi
if [[ "$NO_STATS" -eq 1 ]]; then
  subscriber_cmd+=(--no-stats)
fi

subscriber_env=(RUST_LOG="$LOG_FILTER")
if [[ "$DECODER" == "software" ]]; then
  subscriber_env+=(
    LK_DISABLE_VIDEOTOOLBOX_DECODER=1
    LK_DISABLE_NVDEC=1
  )
fi

redact_command() {
  local item
  local redact_next=0
  for item in "$@"; do
    if [[ "$redact_next" -eq 1 ]]; then
      printf ' <redacted>'
      redact_next=0
      continue
    fi
    printf ' %q' "$item"
    if [[ "$item" == "--api-secret" ]]; then
      redact_next=1
    fi
  done
  printf '\n'
}

{
  echo "name=$NAME"
  echo "room=$NAME"
  echo "duration_seconds=$DURATION"
  echo "warmup_seconds=$WARMUP"
  echo "url=$URL"
  echo "width=$WIDTH"
  echo "height=$HEIGHT"
  echo "fps=$FPS"
  echo "codec=$CODEC"
  echo "encoder=${ENCODER:-default}"
  echo "decoder=$DECODER"
  echo "render_path=$RENDER_PATH"
  echo "headless=$HEADLESS"
  echo "render_vsync=$RENDER_VSYNC"
  echo "keep_window_front=$KEEP_WINDOW_FRONT"
  echo "render_loop_diagnostics=$RENDER_LOOP_DIAGNOSTICS"
  echo "drop_late_frames_ms=$DROP_LATE_FRAMES_MS"
  echo "disable_spotlight_indexing=$DISABLE_SPOTLIGHT"
  echo "test_pattern=$TEST_PATTERN"
  echo "camera_index=$CAMERA_INDEX"
  echo "source=${SOURCE:-default}"
  echo "format=${FORMAT:-default}"
  echo "min_playout_delay_ms=$MIN_PLAYOUT_DELAY"
  echo "max_playout_delay_ms=$MAX_PLAYOUT_DELAY"
  echo "rust_log=$LOG_FILTER"
  echo "require_benchmark_pass=$REQUIRE_BENCHMARK_PASS"
  echo "min_frame_coverage_pct=$MIN_FRAME_COVERAGE_PCT"
  echo "min_time_coverage_pct=$MIN_TIME_COVERAGE_PCT"
  echo "caffeinate_active=$CAFFEINATE_ACTIVE"
  echo "wait_for_idle_host_seconds=$WAIT_FOR_IDLE_HOST"
  echo "idle_confirmation_samples=$IDLE_CONFIRMATION_SAMPLES"
  echo "host_busy_process_cpu_threshold_pct=$HOST_BUSY_PROCESS_CPU_PCT"
  echo "host_busy_external_total_cpu_threshold_pct=$HOST_BUSY_EXTERNAL_TOTAL_CPU_PCT"
  echo "host_load_before=host-load-before.txt"
  echo "host_load_samples=host-load-samples.txt"
  echo "host_load_interval_seconds=$HOST_LOAD_INTERVAL"
  echo "host_load_after=host-load-after.txt"
  [[ -n "$MAX_SINK_GAP_P95_MS" ]] && echo "max_sink_gap_p95_ms=$MAX_SINK_GAP_P95_MS"
  [[ -n "$MAX_PAINT_GAP_P95_MS" ]] && echo "max_paint_gap_p95_ms=$MAX_PAINT_GAP_P95_MS"
  [[ -n "$MAX_E2E_P95_MS" ]] && echo "max_e2e_p95_ms=$MAX_E2E_P95_MS"
  [[ -n "$MAX_RECEIVE_TO_DECODE_P95_MS" ]] && echo "max_receive_to_decode_p95_ms=$MAX_RECEIVE_TO_DECODE_P95_MS"
  [[ -n "$MAX_RECEIVE_TO_PAINT_P95_MS" ]] && echo "max_receive_to_paint_p95_ms=$MAX_RECEIVE_TO_PAINT_P95_MS"
  [[ -n "$MAX_CAPTURE_TO_PACKETIZE_P95_MS" ]] && echo "max_capture_to_packetize_p95_ms=$MAX_CAPTURE_TO_PACKETIZE_P95_MS"
  [[ -n "$MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS" ]] && echo "max_encoder_upload_to_output_p95_ms=$MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS"
  printf 'subscriber_environment='
  redact_command "${subscriber_env[@]}"
  printf 'publisher_command='
  redact_command "${publisher_cmd[@]}"
  printf 'subscriber_command='
  redact_command "${subscriber_cmd[@]}"
} > "$RUN_DIR/metadata.txt"

if ! wait_for_idle_host; then
  read -r top_cpu total_cpu top_name < <(host_load_status)
  capture_host_load "$RUN_DIR/host-load-before.txt"
  capture_host_load "$RUN_DIR/host-load-after.txt"
  write_preflight_failure_artifacts "$top_cpu" "$total_cpu" "$top_name"
  echo "Wrote preflight failure artifacts to $RUN_DIR"
  exit 4
fi

capture_host_load "$RUN_DIR/host-load-before.txt"

pub_pid=""
sub_pid=""
host_load_pid=""
caffeinate_pid=""

start_caffeinate_guard() {
  if [[ "$CAFFEINATE_ACTIVE" -ne 1 ]]; then
    return
  fi

  caffeinate -dimsu -w "$$" &
  caffeinate_pid=$!
  echo "Started macOS caffeinate guard (pid $caffeinate_pid)"
}

stop_caffeinate_guard() {
  if [[ -n "$caffeinate_pid" ]] && kill -0 "$caffeinate_pid" 2>/dev/null; then
    kill -TERM "$caffeinate_pid" 2>/dev/null || true
    wait "$caffeinate_pid" 2>/dev/null || true
  fi
}

start_host_load_sampler() {
  if [[ "$HOST_LOAD_INTERVAL" -eq 0 ]]; then
    return
  fi

  local path="$RUN_DIR/host-load-samples.txt"
  : > "$path"
  (
    while true; do
      append_host_load_sample "$path"
      sleep "$HOST_LOAD_INTERVAL"
    done
  ) &
  host_load_pid=$!
}

stop_host_load_sampler() {
  if [[ -n "$host_load_pid" ]] && kill -0 "$host_load_pid" 2>/dev/null; then
    kill -TERM "$host_load_pid" 2>/dev/null || true
    wait "$host_load_pid" 2>/dev/null || true
  fi
}

stop_processes() {
  local pid
  for pid in "${sub_pid:-}" "${pub_pid:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -INT "$pid" 2>/dev/null || true
    fi
  done

  for _ in 1 2 3 4 5; do
    local any_alive=0
    for pid in "${sub_pid:-}" "${pub_pid:-}"; do
      if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        any_alive=1
      fi
    done
    [[ "$any_alive" -eq 0 ]] && break
    sleep 1
  done

  for pid in "${sub_pid:-}" "${pub_pid:-}"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
    fi
  done
}

cleanup() {
  stop_processes
  stop_host_load_sampler
  stop_caffeinate_guard
}
trap cleanup EXIT INT TERM

echo "Writing benchmark artifacts to $RUN_DIR"
start_caffeinate_guard
start_host_load_sampler
echo "Starting publisher"
(RUST_LOG="$LOG_FILTER" "${publisher_cmd[@]}") > "$RUN_DIR/publisher.log" 2>&1 &
pub_pid=$!

sleep 2

echo "Starting subscriber"
(env "${subscriber_env[@]}" "${subscriber_cmd[@]}") > "$RUN_DIR/subscriber.log" 2>&1 &
sub_pid=$!

total_duration=$((DURATION + WARMUP))
end_time=$((SECONDS + total_duration))
early_exit=0
while [[ "$SECONDS" -lt "$end_time" ]]; do
  if ! kill -0 "$pub_pid" 2>/dev/null; then
    echo "publisher exited before duration elapsed" | tee -a "$RUN_DIR/metadata.txt"
    early_exit=1
    break
  fi
  if ! kill -0 "$sub_pid" 2>/dev/null; then
    echo "subscriber exited before duration elapsed" | tee -a "$RUN_DIR/metadata.txt"
    early_exit=1
    break
  fi
  sleep 1
done

stop_processes
stop_host_load_sampler
stop_caffeinate_guard
trap - EXIT INT TERM
wait "$sub_pid" 2>/dev/null || true
wait "$pub_pid" 2>/dev/null || true
capture_host_load "$RUN_DIR/host-load-after.txt"

analyzer_args=(--name "$NAME" --warmup-seconds "$WARMUP")
analyzer_args+=(
  --min-frame-coverage-pct "$MIN_FRAME_COVERAGE_PCT"
  --min-time-coverage-pct "$MIN_TIME_COVERAGE_PCT"
)
if [[ "$FAIL_ON_STUTTER" -eq 1 ]]; then
  analyzer_args+=(--fail-on-stutter)
fi
if [[ "$REQUIRE_BENCHMARK_PASS" -eq 1 ]]; then
  analyzer_args+=(--require-benchmark-pass)
fi
if [[ -n "$MAX_SINK_GAP_P95_MS" ]]; then
  analyzer_args+=(--max-sink-gap-p95-ms "$MAX_SINK_GAP_P95_MS")
fi
if [[ -n "$MAX_PAINT_GAP_P95_MS" ]]; then
  analyzer_args+=(--max-paint-gap-p95-ms "$MAX_PAINT_GAP_P95_MS")
fi
if [[ -n "$MAX_E2E_P95_MS" ]]; then
  analyzer_args+=(--max-e2e-p95-ms "$MAX_E2E_P95_MS")
fi
if [[ -n "$MAX_RECEIVE_TO_DECODE_P95_MS" ]]; then
  analyzer_args+=(--max-receive-to-decode-p95-ms "$MAX_RECEIVE_TO_DECODE_P95_MS")
fi
if [[ -n "$MAX_RECEIVE_TO_PAINT_P95_MS" ]]; then
  analyzer_args+=(--max-receive-to-paint-p95-ms "$MAX_RECEIVE_TO_PAINT_P95_MS")
fi
if [[ -n "$MAX_CAPTURE_TO_PACKETIZE_P95_MS" ]]; then
  analyzer_args+=(--max-capture-to-packetize-p95-ms "$MAX_CAPTURE_TO_PACKETIZE_P95_MS")
fi
if [[ -n "$MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS" ]]; then
  analyzer_args+=(--max-encoder-upload-to-output-p95-ms "$MAX_ENCODER_UPLOAD_TO_OUTPUT_P95_MS")
fi

"$SCRIPT_DIR/analyze-latency-log.py" "${analyzer_args[@]}"

if [[ "$early_exit" -eq 1 ]]; then
  exit 3
fi
