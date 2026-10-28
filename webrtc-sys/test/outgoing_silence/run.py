#!/usr/bin/env python3
"""Build and run the standalone oracle experiment against a macOS WebRTC artifact."""

import argparse
import shlex
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--webrtc-dir", type=Path, required=True)
    parser.add_argument("--cxx", default="clang++")
    parser.add_argument(
        "--build-dir", type=Path, default=Path(__file__).parent / ".build"
    )
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error(
            "This build recipe supports macOS only (Linux needs matching libc++ flags)."
        )
    root = args.webrtc_dir.resolve()
    for relative in ("webrtc.ninja", "include", "lib/libwebrtc.a"):
        if not (root / relative).exists():
            parser.error(f"Missing artifact path: {root / relative}")
    defines_line = next(
        (
            line
            for line in (root / "webrtc.ninja").read_text().splitlines()
            if line.startswith("defines =")
        ),
        None,
    )
    if defines_line is None:
        parser.error("No defines line in webrtc.ninja")
    # Preserve the artifact's ABI defines, including NDEBUG. Checks in the C++
    # experiment use exceptions rather than assertions so they remain enabled.
    defines = [
        flag
        for flag in shlex.split(defines_line.split("=", 1)[1])
        if not flag.startswith(("-DCR_CLANG_REVISION", "-DCR_XCODE_VERSION"))
    ]
    args.build_dir.mkdir(parents=True, exist_ok=True)
    binary = (args.build_dir / "silence_equivalence").resolve()
    command = [
        args.cxx,
        "-std=c++20",
        "-O2",
        "-Wno-nullability-completeness",
        *defines,
        f"-I{root / 'include'}",
        f"-I{root / 'include/third_party/abseil-cpp'}",
        str(Path(__file__).with_name("silence_equivalence.cc")),
        str(root / "lib/libwebrtc.a"),
        "-o",
        str(binary),
    ]
    for framework in ("Foundation", "Security", "AudioToolbox", "CoreAudio"):
        command.extend(("-framework", framework))
    subprocess.run(command, check=True, stdout=sys.stderr)
    subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
