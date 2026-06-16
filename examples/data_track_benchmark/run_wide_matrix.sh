#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export SIZES="${SIZES:-1,4,16,64,128,256,512}"
export FREQUENCIES="${FREQUENCIES:-1,10,100,1000,5000,10000,50000}"
export DURATION="${DURATION:-3}"
export MAX_EXPECTED_MIBPS="${MAX_EXPECTED_MIBPS:-64}"
export RELIABLE_DRAIN_MS="${RELIABLE_DRAIN_MS:-10000}"
export ROOM="${ROOM:-data-track-benchmark-wide}"
export OUTPUT_DIR="${OUTPUT_DIR:-/private/tmp/livekit-data-track-bench-wide}"

exec "$SCRIPT_DIR/run_local_matrix.sh"
