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

On a pull request, the report job also finds the newest measured commit on the
base branch at or before the PR's merge-base, and compares against it:

    $ ffi_size.py baseline --repo "$GITHUB_REPOSITORY" --workflow ci.yml \
          --base-ref main --head-sha "$PR_HEAD_SHA" --out baseline.json
    $ ffi_size.py compare --head size-summary.json --baseline baseline.json \
          --base-summary base/size-summary.json --markdown report.md

Only the standard library is used, so this runs on every CI runner as is.
"""

import argparse
import glob
import gzip
import json
import os
import shlex
import subprocess
import sys

# A row whose size grows by more than this is flagged in the comparison.
WARN_PERCENT = 1.0

# How far back from the merge-base to look for a measured commit. Pushes to
# main always run the builds, so this only needs to cover a few cancelled or
# failed runs.
MAX_WALK_BACK = 20

SUMMARY_ARTIFACT = "ffi-size-summary"

# Marks the pull request comment, so later runs update it instead of adding
# another one.
COMMENT_MARKER = "<!-- ffi-size-report -->"

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
            data = json.load(f)
        # Anything but a measured row, such as a merged summary from an
        # earlier attempt of the report job, is not an input.
        if isinstance(data, dict) and "row" in data:
            rows.append(data)
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


def gh_api(path):
    out = subprocess.run(["gh", "api", path], capture_output=True, text=True, check=True).stdout
    return json.loads(out)


def find_baseline(api, repo, workflow, base_ref, head_sha, max_walk_back=MAX_WALK_BACK):
    """Find the newest run on `base_ref` that measured the PR's merge-base or
    one of its ancestors. Returns None if there is none within reach."""
    merge_base = api(f"repos/{repo}/compare/{base_ref}...{head_sha}")["merge_base_commit"]["sha"]
    commits = api(f"repos/{repo}/commits?sha={merge_base}&per_page={max_walk_back}")
    for walked, commit in enumerate(commits):
        sha = commit["sha"]
        runs = api(
            f"repos/{repo}/actions/workflows/{workflow}/runs"
            f"?head_sha={sha}&branch={base_ref}&event=push&per_page=10"
        )["workflow_runs"]
        for run in runs:
            artifacts = api(f"repos/{repo}/actions/runs/{run['id']}/artifacts?name={SUMMARY_ARTIFACT}")
            if any(not a["expired"] for a in artifacts["artifacts"]):
                return {
                    "run_id": run["id"],
                    "sha": sha,
                    "merge_base": merge_base,
                    "walked": walked,
                    "base_ref": base_ref,
                }
    return None


def baseline(args):
    try:
        found = find_baseline(gh_api, args.repo, args.workflow, args.base_ref, args.head_sha)
    except (subprocess.CalledProcessError, KeyError, ValueError) as e:
        # A missing baseline only weakens the report; it must not fail CI.
        print(f"baseline lookup failed: {e}", file=sys.stderr)
        found = None
    with open(args.out, "w") as f:
        json.dump(found, f, indent=2)
        f.write("\n")
    print(json.dumps(found, indent=2))


def signed(n):
    return ("+" if n > 0 else "-" if n < 0 else "") + human(abs(n))


def render_comparison(head, base, found):
    base_rows = {(r["row"], r["target"]): r for r in (base or {}).get("rows", [])}
    head_rows = {(r["row"], r["target"]): r for r in head["rows"]}
    base_label = f"`{found['base_ref']}` `{found['sha'][:7]}`" if found else "base"

    table = [
        f"| | Build | Target | {base_label} | This PR | Δ | Δ% |",
        "|---|---|---|---:|---:|---:|---:|",
    ]
    largest = None
    for key in sorted(set(base_rows) | set(head_rows)):
        b, h = base_rows.get(key), head_rows.get(key)
        row, target = key
        if h is None:
            table.append(f"| ❌ | {row} | `{target}` | {human(b['bytes'])} | _not measured_ | | |")
            continue
        if b is None:
            table.append(f"| 🆕 | {row} | `{target}` | | {human(h['bytes'])} | | |")
            continue
        delta = h["bytes"] - b["bytes"]
        percent = 100.0 * delta / b["bytes"] if b["bytes"] else 0.0
        flag = "⚠️" if percent > WARN_PERCENT else ""
        table.append(
            f"| {flag} | {row} | `{target}` | {human(b['bytes'])} | {human(h['bytes'])} "
            f"| {signed(delta)} | {percent:+.2f}% |"
        )
        if delta and (largest is None or abs(delta) > abs(largest[0])):
            largest = (delta, percent, row, target)

    lines = [COMMENT_MARKER, "## livekit-ffi build sizes", ""]
    if not found or base is None:
        lines.append(
            f"No measured commit found on the base branch within {MAX_WALK_BACK} commits "
            f"of the merge-base, so there is nothing to compare against yet."
        )
    elif largest is None:
        lines.append("No size changes.")
    else:
        delta, percent, row, target = largest
        lines.append(f"Largest change: {row} `{target}` {signed(delta)} ({percent:+.2f}%).")
    lines += ["", *table, ""]

    measured = describe_commit(head)
    if found:
        note = f"Compared with `{found['base_ref']}` at `{found['sha'][:7]}`"
        if found["walked"]:
            note += (
                f", the nearest measured ancestor of the merge-base `{found['merge_base'][:7]}` "
                f"({found['walked']} commits back)"
            )
        else:
            note += ", the merge-base"
        measured = f"{measured}. {note}." if measured else f"{note}."
    if measured:
        lines.append(f"<sub>{measured} Raw file sizes; ⚠️ marks growth over {WARN_PERCENT:g}%.</sub>")
    return "\n".join(lines) + "\n"


def compare(args):
    with open(args.head) as f:
        head = json.load(f)
    found = None
    if args.baseline and os.path.isfile(args.baseline):
        with open(args.baseline) as f:
            found = json.load(f)
    base = None
    if args.base_summary and os.path.isfile(args.base_summary):
        with open(args.base_summary) as f:
            base = json.load(f)
    markdown = render_comparison(head, base, found)
    with open(args.markdown, "w") as f:
        f.write(markdown)
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

    b = sub.add_parser("baseline", help="find the base branch run to compare a PR against")
    b.add_argument("--repo", required=True)
    b.add_argument("--workflow", required=True, help="file name of the workflow whose runs hold the sizes")
    b.add_argument("--base-ref", required=True)
    b.add_argument("--head-sha", required=True)
    b.add_argument("--out", required=True, help="where to write the run found, or null")
    b.set_defaults(func=baseline)

    c = sub.add_parser("compare", help="render a PR's sizes against a baseline")
    c.add_argument("--head", required=True, help="this run's merged summary")
    c.add_argument("--baseline", help="output of `baseline`")
    c.add_argument("--base-summary", help="the baseline run's merged summary")
    c.add_argument("--markdown", required=True, help="write the comment body here")
    c.set_defaults(func=compare)

    args = ap.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
