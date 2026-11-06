#!/usr/bin/env python3
"""Build standalone silence-scheduler tests against a matching WebRTC artifact."""

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
        "--case", choices=("runtime", "queue", "factory"), default="runtime"
    )
    parser.add_argument("--sanitize", action="store_true")
    parser.add_argument(
        "--build-dir", type=Path, default=Path(__file__).parent / ".build"
    )
    args = parser.parse_args()
    if sys.platform not in ("darwin", "linux"):
        parser.error("This standalone build recipe supports macOS and Linux")
    root = args.webrtc_dir.resolve()
    for relative in (
        "webrtc.ninja",
        "include/audio/silence_encoder_task_queue.h",
        "lib/libwebrtc.a",
    ):
        if not (root / relative).exists():
            parser.error(
                f"Missing patched artifact path: {root / relative}; rebuild WebRTC first"
            )
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
    # Keep ABI defines (including NDEBUG). Tests use explicit runtime checks.
    defines = [
        flag
        for flag in shlex.split(defines_line.split("=", 1)[1])
        if not flag.startswith(("-DCR_CLANG_REVISION", "-DCR_XCODE_VERSION"))
    ]
    name = {
        "runtime": "runtime_equivalence",
        "queue": "silence_task_queue_test",
        "factory": "codec_factory_test",
    }[args.case]
    args.build_dir.mkdir(parents=True, exist_ok=True)
    binary = (args.build_dir / name).resolve()
    command = [
        args.cxx,
        "-std=c++20",
        "-O2",
        "-fno-rtti",
        "-Wno-nullability-completeness",
        *defines,
        f"-I{root / 'include'}",
        f"-I{root / 'include/third_party/abseil-cpp'}",
    ]
    if sys.platform == "linux":
        # Mirror webrtc-sys/build.rs: Chromium's libc++ ABI uses std::__Cr and
        # trivial_abi, so both its headers and a sufficiently recent clang are required.
        libcxx = root / "include/third_party/libc++/src/include"
        if not (libcxx / "span").exists():
            parser.error(
                "Linux artifact is missing the matching hermetic libc++ headers"
            )
        command.extend(
            [
                "-nostdinc++",
                f"-isystem{libcxx}",
                f"-isystem{root / 'include/third_party/libc++abi/src/include'}",
                f"-I{root / 'include/buildtools/third_party/libc++'}",
                "-nostdlib++",
            ]
        )
    if args.sanitize:
        command.extend(["-fsanitize=address,undefined", "-fno-omit-frame-pointer"])
    command.extend(
        [
            str(Path(__file__).with_name(f"{name}.cc")),
            str(root / "lib/libwebrtc.a"),
            "-o",
            str(binary),
        ]
    )
    if sys.platform == "darwin":
        for framework in ("Foundation", "Security", "AudioToolbox", "CoreAudio"):
            command.extend(("-framework", framework))
    else:
        # libc++ and libc++abi implementations are already archived into libwebrtc.a.
        command.extend(["-ldl", "-lpthread", "-lrt", "-lm"])
    subprocess.run(command, check=True, stdout=sys.stderr)
    subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
