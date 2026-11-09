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

cargo_target_dir="${CARGO_TARGET_DIR:-$PWD/target}"
shopt -s nullglob
webrtc_archives=(
  "$cargo_target_dir"/debug/build/scratch-*/out/livekit_webrtc/livekit/linux-x64-release-*/linux-x64-release/lib/libwebrtc.a
)
shopt -u nullglob

if [ "${#webrtc_archives[@]}" -ne 1 ]; then
  echo "Benchmark setup failed: expected one downloaded libwebrtc archive, found ${#webrtc_archives[@]}" >&2
  exit 1
fi

webrtc_root="${webrtc_archives[0]%/lib/libwebrtc.a}"
benchmark_build_dir="${BENCHMARK_BUILD_DIR:-/tmp/webrtc-sys-benchmark}"

cmake \
  -S webrtc-sys/test \
  -B "$benchmark_build_dir" \
  -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DWEBRTC_ROOT="$webrtc_root"
cmake --build "$benchmark_build_dir" --target h264_benchmark

fixture_dir="$benchmark_build_dir/resources"
fixture="$fixture_dir/FourPeople_1280x720_30.yuv"
mkdir -p "$fixture_dir"
python3 - "$fixture" <<'PY'
import pathlib
import sys

output = pathlib.Path(sys.argv[1])
width = 1280
height = 720
frames = 30
chroma_width = width // 2
chroma_height = height // 2

with output.open("wb") as stream:
    for frame in range(frames):
        y_row = bytes((x + frame * 7) % 256 for x in range(width))
        u_row = bytes((96 + x // 8 + frame * 3) % 256 for x in range(chroma_width))
        v_row = bytes((160 - x // 8 + frame * 5) % 256 for x in range(chroma_width))
        for _ in range(height):
            stream.write(y_row)
        for _ in range(chroma_height):
            stream.write(u_row)
        for _ in range(chroma_height):
            stream.write(v_row)
PY

benchmark_log="$benchmark_build_dir/nvidia-benchmark.log"
echo "BENCHMARK_FIXTURE kind=synthetic-i420 resolution=1280x720 frames=30"
echo "Benchmark throughput is diagnostic only; this synthetic fixture is not a performance baseline."
(
  cd "$benchmark_build_dir"
  ./h264_benchmark 2>&1 | tee "$benchmark_log"
)

if ! grep -q 'NvidiaBenchmark' "$benchmark_log"; then
  echo "NVIDIA benchmark failed: the existing harness skipped the NVIDIA backend" >&2
  exit 1
fi

if ! grep -q 'OpenH264Benchmark' "$benchmark_log"; then
  echo "OpenH264 benchmark failed: the existing harness did not run it" >&2
  exit 1
fi

if [ ! -s "$benchmark_build_dir/nvidia_bitstream_output.h264" ]; then
  echo "NVIDIA benchmark failed: encoded H.264 output is empty" >&2
  exit 1
fi

if [ ! -s "$benchmark_build_dir/openh264_bitstream_output.h264" ]; then
  echo "OpenH264 benchmark failed: encoded H.264 output is empty" >&2
  exit 1
fi
