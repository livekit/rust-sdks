#!/usr/bin/env bash

set -euo pipefail

if ! gpu_list="$(nvidia-smi -L 2>&1)"; then
  echo "GPU check failed: nvidia-smi could not query a GPU" >&2
  echo "$gpu_list" >&2
  exit 1
fi

echo "$gpu_list"
if ! grep -q '^GPU [0-9]\+:' <<<"$gpu_list"; then
  echo "GPU check failed: no NVIDIA GPU was reported" >&2
  exit 1
fi

build_log="$(mktemp)"
trap 'rm -f "$build_log"' EXIT

if ! cargo build -p webrtc-sys 2>&1 | tee "$build_log"; then
  echo "webrtc-sys build failed" >&2
  exit 1
fi

if grep -Fq 'cuda.h not found' "$build_log"; then
  echo "NVENC check failed: webrtc-sys built without CUDA headers" >&2
  exit 1
fi

cargo test -p webrtc-sys -p libwebrtc
