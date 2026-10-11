#!/usr/bin/env python3
# Copyright 2026 LiveKit, Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Generates `passthrough_streams.rs`: tiny encoded streams for passthrough tests.

Each stream is one keyframe followed by delta frames, stored as access units in the
form the passthrough encoder forwards unchanged, so a loopback can compare bytes:

- H264/H265: Annex B with a 4-byte start code before every NAL unit (the form the
  receive side reassembles), without AUD and SEI NAL units.
- AV1: low-overhead OBUs without temporal delimiters (the sender strips them).
- VP8/VP9: one frame per access unit (no alt-ref, no superframes).

Usage: python3 gen_passthrough_streams.py > passthrough_streams.rs
Requires ffmpeg/ffprobe with libx264, libx265, libvpx and libaom.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

FRAMES = 5

# name, codec label, width, height, ffmpeg encoder args, container
STREAMS = [
    ("H264", "H264", 64, 64,
     ["-c:v", "libx264", "-profile:v", "baseline", "-bf", "0", "-g", "300",
      "-x264-params", "aud=1"], "h264"),
    ("H264_128X72", "H264", 128, 72,
     ["-c:v", "libx264", "-profile:v", "baseline", "-bf", "0", "-g", "300",
      "-x264-params", "aud=1"], "h264"),
    ("H265", "H265", 64, 64,
     ["-c:v", "libx265", "-preset", "ultrafast", "-tune", "zerolatency",
      "-x265-params", "aud=1:keyint=300:bframes=0:log-level=error"], "hevc"),
    ("VP8", "VP8", 64, 64,
     ["-c:v", "libvpx", "-deadline", "realtime", "-auto-alt-ref", "0",
      "-lag-in-frames", "0", "-g", "300"], "ivf"),
    ("VP9", "VP9", 64, 64,
     ["-c:v", "libvpx-vp9", "-deadline", "realtime", "-auto-alt-ref", "0",
      "-lag-in-frames", "0", "-g", "300", "-row-mt", "0"], "ivf"),
    ("AV1", "AV1", 64, 64,
     ["-c:v", "libaom-av1", "-usage", "realtime", "-cpu-used", "8",
      "-lag-in-frames", "0", "-g", "300"], "ivf"),
]


def run(cmd):
    return subprocess.run(cmd, check=True, capture_output=True).stdout


def tool_version(tool):
    return run([tool, "-version"]).decode().splitlines()[0]


def split_annex_b(data):
    """Returns NAL unit payloads (without start codes)."""
    nals, i, start = [], 0, None
    while i + 3 <= len(data):
        if data[i:i + 3] == b"\x00\x00\x01":
            if start is not None:
                nals.append(data[start:i].rstrip(b"\x00"))
            start = i + 3
            i += 3
        else:
            i += 1
    if start is not None:
        nals.append(data[start:])
    return nals


def annex_b_units(data, codec):
    """Groups NAL units into access units at AUD boundaries, dropping AUD and SEI."""
    if codec == "H264":
        nal_type = lambda n: n[0] & 0x1F
        aud, drop, key = 9, {6, 9}, {5}
    else:
        nal_type = lambda n: (n[0] >> 1) & 0x3F
        aud, drop, key = 35, {35, 39, 40}, set(range(16, 22))
    units, current = [], None
    for nal in split_annex_b(data):
        if nal_type(nal) == aud:
            if current:
                units.append(current)
            current = []
            continue
        if current is None:
            current = []
        if nal_type(nal) not in drop:
            current.append(nal)
    if current:
        units.append(current)
    out = []
    for nals in units:
        is_key = any(nal_type(n) in key for n in nals)
        out.append((is_key, b"".join(b"\x00\x00\x00\x01" + n for n in nals)))
    return out


def ivf_frames(data):
    assert data[:4] == b"DKIF", "not an IVF file"
    header_len = int.from_bytes(data[6:8], "little")
    i, frames = header_len, []
    while i + 12 <= len(data):
        size = int.from_bytes(data[i:i + 4], "little")
        frames.append(data[i + 12:i + 12 + size])
        i += 12 + size
    return frames


def read_leb128(data, i):
    value, shift = 0, 0
    while True:
        b = data[i]
        value |= (b & 0x7F) << shift
        i += 1
        if not b & 0x80:
            return value, i
        shift += 7


def strip_temporal_delimiters(tu):
    out, i = b"", 0
    while i < len(tu):
        header = tu[i]
        obu_type = (header >> 3) & 0xF
        has_ext = (header >> 2) & 1
        assert (header >> 1) & 1, "OBU without size field"
        size, payload_start = read_leb128(tu, i + 1 + has_ext)
        end = payload_start + size
        if obu_type != 2:
            out += tu[i:end]
        i = end
    return out


def key_flags(path):
    out = run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
               "packet=flags", "-of", "csv=p=0", str(path)]).decode().split()
    return ["K" in f for f in out]


def generate(name, codec, width, height, args, container, tmp):
    path = Path(tmp) / f"{name}.{container}"
    run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
         f"testsrc2=size={width}x{height}:rate=30", "-frames:v", str(FRAMES),
         "-pix_fmt", "yuv420p", *args, "-f", container, str(path)])
    data = path.read_bytes()
    if codec in ("H264", "H265"):
        units = annex_b_units(data, codec)
    else:
        flags = key_flags(path)
        frames = ivf_frames(data)
        assert len(flags) == len(frames), f"{name}: {len(flags)} flags, {len(frames)} frames"
        if codec == "AV1":
            frames = [strip_temporal_delimiters(f) for f in frames]
        units = list(zip(flags, frames))
    assert len(units) == FRAMES, f"{name}: expected {FRAMES} units, got {len(units)}"
    assert units[0][0] and not any(k for k, _ in units[1:]), f"{name}: expected K then deltas"
    return units


def rust_bytes(data):
    rows = [", ".join(f"0x{b:02x}" for b in data[i:i + 16]) for i in range(0, len(data), 16)]
    return "&[\n" + "".join(f"            {r},\n" for r in rows) + "        ]"


def main():
    print("// Generated by gen_passthrough_streams.py. Do not edit by hand.")
    print(f"// {tool_version('ffmpeg')}")
    print("// Each stream: one keyframe followed by delta frames, see the generator docstring.")
    print()
    print("#[allow(dead_code)]")
    print("pub struct TestStream {")
    print("    pub codec: &'static str,")
    print("    pub width: u32,")
    print("    pub height: u32,")
    print("    /// `(is_keyframe, access_unit)` in decode order.")
    print("    pub units: &'static [(bool, &'static [u8])],")
    print("}")
    names = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, codec, width, height, args, container in STREAMS:
            units = generate(name, codec, width, height, args, container, tmp)
            names.append(name)
            print()
            print("#[allow(dead_code)]")
            print(f"pub const {name}: TestStream = TestStream {{")
            print(f"    codec: \"{codec}\",")
            print(f"    width: {width},")
            print(f"    height: {height},")
            print("    units: &[")
            for is_key, unit in units:
                print(f"        ({'true' if is_key else 'false'}, {rust_bytes(unit)}),")
            print("    ],")
            print("};")
    print()
    print("#[allow(dead_code)]")
    print(f"pub const ALL: &[&TestStream] = &[{', '.join('&' + n for n in names)}];")


if __name__ == "__main__":
    sys.exit(main())
