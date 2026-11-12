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

"""Measure livekit-ffi build artifacts and summarize them.

builds.yml builds livekit-ffi once per target in the configuration a platform
ships (see [package.metadata.platform-features] in livekit-ffi/Cargo.toml),
then records the size of the library it produced:

    $ ffi_size.py measure --row room-apis --target aarch64-apple-ios \\
          --flags "$FFI_FLAGS" --out sizes/room-apis-aarch64-apple-ios.json

The report job merges every row into one file, which is kept as the
`ffi-size-summary` artifact, and renders it as markdown:

    $ ffi_size.py summary sizes/ --json size-summary.json --sha "$GITHUB_SHA"

On a pull request, $GITHUB_SHA is the merge commit that was checked out and
measured, not the PR head; pass the head as --pr-head-sha so the summary can
name both.

Only the standard library is used, so this runs on every CI runner as is.
"""

import argparse
import glob
import gzip
import json
import os
import shlex
import sys

# The file cargo writes for each crate type, by target OS.
ARTIFACT_NAMES = {
    ("windows", "cdylib"): "livekit_ffi.dll",
    ("windows", "staticlib"): "livekit_ffi.lib",
    ("apple", "cdylib"): "liblivekit_ffi.dylib",
    ("apple", "staticlib"): "liblivekit_ffi.a",
    ("unix", "cdylib"): "liblivekit_ffi.so",
    ("unix", "staticlib"): "liblivekit_ffi.a",
}


def target_os(target):
    if "windows" in target:
        return "windows"
    if "apple" in target:
        return "apple"
    return "unix"


def crate_type_from_flags(flags):
    args = shlex.split(flags)
    for i, arg in enumerate(args):
        if arg == "--crate-type" and i + 1 < len(args):
            return args[i + 1]
    sys.exit(
        f"no --crate-type in flags {flags!r}; without one cargo emits every "
        f"crate type in livekit-ffi's [lib], including the rlib, which drops LTO"
    )


def artifact_path(target_dir, target, profile, crate_type):
    name = ARTIFACT_NAMES[(target_os(target), crate_type)]
    return os.path.join(target_dir, target, profile, name)


def gzip_size(path):
    with open(path, "rb") as f:
        # mtime=0 so the same input always compresses to the same size.
        return len(gzip.compress(f.read(), compresslevel=9, mtime=0))


def measure(args):
    crate_type = crate_type_from_flags(args.flags)
    path = artifact_path(args.target_dir, args.target, args.profile, crate_type)
    if not os.path.isfile(path):
        sys.exit(f"artifact not found at {path}")

    row = {
        "row": args.row,
        "target": args.target,
        "crate_type": crate_type,
        "flags": args.flags,
        "file": os.path.basename(path),
        "bytes": os.path.getsize(path),
        "gzip_bytes": gzip_size(path),
    }
    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(row, f, indent=2)
        f.write("\n")
    print(json.dumps(row, indent=2))


def load_rows(directory):
    rows = []
    for path in sorted(glob.glob(os.path.join(directory, "**", "*.json"), recursive=True)):
        with open(path) as f:
            rows.append(json.load(f))
    rows.sort(key=lambda r: (r["row"], r["target"]))
    return rows


def human(n):
    for unit in ("B", "KiB", "MiB"):
        if abs(n) < 1024 or unit == "MiB":
            return f"{n:.0f} {unit}" if unit == "B" else f"{n:.2f} {unit}"
        n /= 1024


def describe_commit(summary):
    sha = summary.get("sha", "")[:7]
    head = summary.get("pr_head_sha", "")[:7]
    if sha and head:
        return f"Merge commit `{sha}` of PR head `{head}`"
    return f"Commit `{sha}`" if sha else ""


def render(summary):
    lines = [
        "## livekit-ffi build sizes",
        "",
        describe_commit(summary),
        "",
        "| Build | Target | Type | Size | gzip |",
        "|---|---|---|---:|---:|",
    ]
    for r in summary["rows"]:
        lines.append(
            f"| {r['row']} | `{r['target']}` | {r['crate_type']} "
            f"| {human(r['bytes'])} | {human(r['gzip_bytes'])} |"
        )
    if not summary["rows"]:
        lines.append("| _no builds were measured_ | | | | |")
    return "\n".join(lines) + "\n"


def summary(args):
    result = {"sha": args.sha}
    if args.pr_head_sha:
        result["pr_head_sha"] = args.pr_head_sha
    result["rows"] = load_rows(args.directory)
    if args.json:
        with open(args.json, "w") as f:
            json.dump(result, f, indent=2)
            f.write("\n")
    markdown = render(result)
    if args.markdown:
        with open(args.markdown, "a") as f:
            f.write(markdown)
    else:
        sys.stdout.write(markdown)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="command", required=True)

    m = sub.add_parser("measure", help="record the size of one built artifact")
    m.add_argument("--row", required=True, help="row name, e.g. room-apis or core-modules")
    m.add_argument("--target", required=True, help="rust target triple")
    m.add_argument("--flags", required=True, help="the cargo flags the artifact was built with")
    m.add_argument("--target-dir", default="target")
    m.add_argument("--profile", default="release")
    m.add_argument("--out", required=True)
    m.set_defaults(func=measure)

    s = sub.add_parser("summary", help="merge measured rows and render markdown")
    s.add_argument("directory")
    s.add_argument("--sha", default="", help="the commit that was checked out and measured")
    s.add_argument("--pr-head-sha", default="", help="on a pull request, the head merged into --sha")
    s.add_argument("--json", help="write the merged rows here")
    s.add_argument("--markdown", help="append the markdown here instead of printing it")
    s.set_defaults(func=summary)

    args = ap.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
