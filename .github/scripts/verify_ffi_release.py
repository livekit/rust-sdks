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

"""Check a published livekit-ffi GitHub release as its users get it.

The last job of a livekit-ffi release run calls this script, once every
workflow that attaches files to the release has finished. It reads the
release with the `gh` CLI and fails if the release is not good:

- The release is a draft, or the tag does not point at --expected-sha.
- A file is missing, is not fully uploaded, or is empty. The expected files
  come from the build matrices, not from a list kept by hand:
    ffi-builds.yml   -> ffi-<name>.zip for each target, ffi-python-bindings.zip
    uniffi-cdylib.yml -> build-<target>.zip and build-<target>.zip.sha256
- --required-assets (REQUIRED_ASSETS in ffi-builds.yml) is not equal to the
  ffi-*.zip files of the matrix. That list gates the publish, so a target
  missing from it could make a release public without its library.
- A zip does not open, or lacks the files that its consumers read.
- A build-*.zip does not match its .sha256 sidecar, which the Dart build hook
  checks at consumer build time.

The matrices are passed as JSON (`yq -o=json` of `.jobs.build.strategy.matrix.include`),
so this script needs only the standard library.

Exit status 1 on any error. Each error is also an `::error::` annotation, and
the result is written to $GITHUB_STEP_SUMMARY when it is set.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

PYTHON_BINDINGS = "ffi-python-bindings.zip"

errors: list[str] = []
warnings: list[str] = []


def error(msg: str) -> None:
    errors.append(msg)
    print(f"::error::{msg}", flush=True)


def warning(msg: str) -> None:
    warnings.append(msg)
    print(f"::warning::{msg}", flush=True)


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], check=True, capture_output=True, text=True).stdout


def cdylib_name(target: str) -> str:
    # Same mapping as the packaging steps of uniffi-cdylib.yml.
    if "apple" in target:
        return "liblivekit_ffi.dylib"
    if "windows" in target:
        return "livekit_ffi.dll"
    return "liblivekit_ffi.so"


def expected_assets(ffi_matrix: list[dict], cdylib_matrix: list[dict]) -> dict[str, set[str]]:
    """Map each expected file to the names that must be inside it (empty for a sidecar)."""
    expected: dict[str, set[str]] = {}
    for entry in ffi_matrix:
        members = {entry["dylib"], "livekit_ffi.h", "LICENSE.md"}
        if entry.get("jar"):
            members.add(entry["jar"])
        expected[f"{entry['name']}.zip"] = members
    expected[PYTHON_BINDINGS] = {"livekit_ffi.py", "LICENSE.md"}
    for entry in cdylib_matrix:
        zip_name = f"build-{entry['target']}.zip"
        expected[zip_name] = {cdylib_name(entry["target"])}
        expected[f"{zip_name}.sha256"] = set()
    return expected


def check_required_list(required: str, ffi_matrix: list[dict]) -> None:
    listed = set(required.split())
    derived = {f"{e['name']}.zip" for e in ffi_matrix} | {PYTHON_BINDINGS}
    for name in sorted(derived - listed):
        error(
            f"REQUIRED_ASSETS in ffi-builds.yml lacks {name}: the publish does not wait for it"
        )
    for name in sorted(listed - derived):
        error(f"REQUIRED_ASSETS in ffi-builds.yml lists {name}, which no build target makes")


def check_release(tag: str, expected_sha: str | None) -> list[dict] | None:
    try:
        release = json.loads(gh("release", "view", tag, "--json", "isDraft,assets"))
    except subprocess.CalledProcessError as e:
        # A token with read access cannot see a draft, so a draft lands here too.
        error(f"cannot read the release {tag}: it does not exist or is not public. {e.stderr.strip()}")
        return None
    if release["isDraft"]:
        error(f"the release {tag} is still a draft")
    if expected_sha:
        repo = os.environ.get("GH_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
        tagged = gh("api", f"repos/{repo}/commits/{tag}", "--jq", ".sha").strip()
        if tagged != expected_sha:
            error(f"the tag {tag} points at {tagged}, not at the released commit {expected_sha}")
    return release["assets"]


def check_listing(assets: list[dict], expected: dict[str, set[str]]) -> list[str]:
    """Check the release listing. Returns the expected files that can be downloaded."""
    on_release = {a["name"]: a for a in assets}
    present = []
    for name in sorted(expected):
        asset = on_release.get(name)
        if asset is None:
            error(f"missing: {name}")
        elif asset.get("state", "uploaded") != "uploaded":
            error(f"not fully uploaded ({asset['state']}): {name}")
        elif asset["size"] == 0:
            error(f"empty: {name}")
        else:
            present.append(name)
    for name in sorted(set(on_release) - set(expected)):
        warning(f"not made by any build target: {name}")
    return present


def check_contents(tag: str, present: list[str], expected: dict[str, set[str]]) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        patterns = [arg for name in present for arg in ("--pattern", name)]
        gh("release", "download", tag, "--dir", tmp, *patterns)
        dest = Path(tmp)
        for name in present:
            path = dest / name
            if name.endswith(".zip"):
                check_zip(path, expected[name])
            elif name.endswith(".zip.sha256"):
                check_sidecar(path, dest / name.removesuffix(".sha256"))


def check_zip(path: Path, members: set[str]) -> None:
    try:
        with zipfile.ZipFile(path) as archive:
            bad = archive.testzip()
            if bad is not None:
                error(f"{path.name}: {bad} is corrupt")
            # Full paths, not base names: consumers (the Dart build hook among
            # them) read each file at the root of the archive.
            names = {n for n in archive.namelist() if not n.endswith("/")}
    except zipfile.BadZipFile as e:
        error(f"{path.name}: not a zip ({e})")
        return
    for member in sorted(members - names):
        error(f"{path.name}: lacks {member}")


def check_sidecar(sidecar: Path, archive: Path) -> None:
    # `<lowercase hex>  <file name>`, the format that the Dart build hook parses.
    fields = sidecar.read_text(encoding="ascii", errors="replace").split()
    if len(fields) != 2:
        error(f"{sidecar.name}: expected '<sha256>  <file>', got {sidecar.read_text()!r}")
        return
    digest, filename = fields
    if filename != archive.name:
        error(f"{sidecar.name}: names {filename}, not {archive.name}")
    if not archive.exists():
        return  # reported as missing already
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != actual:
        error(f"{sidecar.name}: says {digest}, but {archive.name} has {actual}")


def write_summary(tag: str, expected: dict[str, set[str]]) -> None:
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    lines = [f"## livekit-ffi release {tag}", ""]
    if errors:
        lines += [f"**Not good: {len(errors)} error(s).**", ""]
        lines += [f"- {e}" for e in errors]
    else:
        lines += [f"Good: the release is public with all {len(expected)} expected files."]
    if warnings:
        lines += ["", "Warnings:", ""] + [f"- {w}" for w in warnings]
    with open(path, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--tag", required=True, help="e.g. livekit-ffi/v0.14.0")
    parser.add_argument("--ffi-matrix", required=True, type=Path, help="JSON, ffi-builds.yml")
    parser.add_argument("--cdylib-matrix", required=True, type=Path, help="JSON, uniffi-cdylib.yml")
    parser.add_argument("--required-assets", required=True, help="REQUIRED_ASSETS of ffi-builds.yml")
    parser.add_argument("--expected-sha", help="The commit that the tag must point at")
    args = parser.parse_args()

    ffi_matrix = json.loads(args.ffi_matrix.read_text())
    cdylib_matrix = json.loads(args.cdylib_matrix.read_text())
    if not ffi_matrix or not cdylib_matrix:
        error("a build matrix is empty: the yq query no longer matches the workflow")
        return 1
    expected = expected_assets(ffi_matrix, cdylib_matrix)

    check_required_list(args.required_assets, ffi_matrix)
    assets = check_release(args.tag, args.expected_sha)
    if assets is not None:
        present = check_listing(assets, expected)
        if present:
            check_contents(args.tag, present, expected)

    write_summary(args.tag, expected)
    if errors:
        print(f"{args.tag}: {len(errors)} error(s).")
        return 1
    print(f"{args.tag}: good. {len(expected)} files checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
