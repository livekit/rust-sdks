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

"""Detect which knope-managed packages a PR affects and validate changeset coverage.

A changed crate requires a version bump not just for itself, but for every crate
that (transitively) depends on it. This computes that closure from `cargo metadata`
and compares it against the bumps already declared in the PR's changeset files.

Inputs (environment variables):
  CHANGED_FILES     newline-separated list of files changed by the PR
  CHANGESET_FILES   newline-separated list of .changeset/*.md files in the PR diff
  PR_TITLE          PR title, used for the prefilled changeset description (optional)
  PR_NUMBER         PR number (optional)
  PR_AUTHOR         PR author login (optional)

Output (JSON on stdout):
  {
    "error":             str,               # non-empty if cargo metadata failed
    "direct":            [pkg, ...],         # packages whose own files changed
    "downstream":        [pkg, ...],         # transitive dependents of `direct`
    "required":          [pkg, ...],         # direct + downstream (full closure)
    "present":           [[pkg, bump], ...], # bumps already in the changeset
    "missing":           [pkg, ...],         # required packages not yet covered
    "changeset_content": str,               # a changeset prefilled with `missing`
  }
"""

import json
import os
import re
import subprocess
import sys

BUMP_ORDER = {"patch": 0, "minor": 1, "major": 2}


def emit(data):
    """Write the result as JSON and exit successfully."""
    json.dump(data, sys.stdout)
    sys.exit(0)


def read_lines(env_var):
    return [line.strip() for line in os.environ.get(env_var, "").strip().split("\n") if line.strip()]


def load_knope_packages():
    """Return the set of package names managed by knope (from knope.toml)."""
    with open("knope.toml") as f:
        return set(re.findall(r"^\[packages\.([^\]]+)\]", f.read(), re.MULTILINE))


def parse_present_bumps(changeset_files, knope_packages):
    """Parse the highest bump declared per package across the PR's changeset files.

    Changeset front matter looks like:  "livekit-api": patch
    """
    present = {}
    for filepath in changeset_files:
        try:
            with open(filepath) as f:
                content = f.read()
        except OSError:
            continue
        match = re.match(r"^---\s*\n(.*?)\n---", content, re.DOTALL)
        if not match:
            continue
        for line in match.group(1).strip().split("\n"):
            line = line.strip()
            if ":" not in line:
                continue
            pkg, bump = line.split(":", 1)
            pkg = pkg.strip().strip('"').strip("'").strip()
            bump = bump.strip().strip('"').strip("'").strip()
            if bump in BUMP_ORDER and pkg in knope_packages:
                if pkg not in present or BUMP_ORDER[bump] > BUMP_ORDER[present[pkg]]:
                    present[pkg] = bump
    return present


def build_reverse_dep_graph(meta, knope_packages):
    """Map each knope package to the set of knope packages that directly depend on it."""
    reverse = {name: set() for name in knope_packages}
    for pkg in meta["packages"]:
        if pkg["name"] not in knope_packages:
            continue
        # dependencies includes both normal and build deps
        for dep in pkg.get("dependencies", []):
            if dep["name"] in knope_packages:
                reverse[dep["name"]].add(pkg["name"])
    return reverse


def transitive_downstream(pkg, reverse):
    """All packages that (transitively) depend on `pkg`."""
    visited = set()
    stack = list(reverse.get(pkg, set()))
    while stack:
        node = stack.pop()
        if node not in visited:
            visited.add(node)
            stack.extend(reverse.get(node, set()) - visited)
    return visited


def match_changed_packages(changed_files, pkg_to_dir):
    """Map changed files to knope packages (longest directory prefix wins)."""
    sorted_pkgs = sorted(pkg_to_dir.items(), key=lambda x: len(x[1]), reverse=True)
    direct = set()
    for f in changed_files:
        for pkg_name, pkg_dir in sorted_pkgs:
            if f.startswith(pkg_dir + "/"):
                direct.add(pkg_name)
                break
    return direct


def build_changeset_content(missing, pr_title=None, pr_number=None, pr_author=None):
    """Build a changeset that fills in `patch` bumps for the missing packages."""
    pr_title = pr_title if pr_title is not None else os.environ.get("PR_TITLE", "Description of your change")
    pr_number = pr_number if pr_number is not None else os.environ.get("PR_NUMBER", "")
    pr_author = pr_author if pr_author is not None else os.environ.get("PR_AUTHOR", "")
    lines = ["---"]
    for pkg in missing:
        lines.append(f'"{pkg}": patch')
    lines.extend(["---", "", f"{pr_title} - #{pr_number} (@{pr_author})"])
    return "\n".join(lines)


def package_directories(meta, knope_packages):
    """Map each knope package name to its workspace-relative directory."""
    workspace_root = meta["workspace_root"]
    pkg_to_dir = {}
    for pkg in meta["packages"]:
        if pkg["name"] in knope_packages:
            manifest_dir = os.path.dirname(pkg["manifest_path"])
            pkg_to_dir[pkg["name"]] = os.path.relpath(manifest_dir, workspace_root)
    return pkg_to_dir


def detect(meta, knope_packages, changed_files, present):
    """Compute affected packages and changeset coverage. Pure — no I/O or cargo.

    `present` maps already-declared package -> bump. Returns the result dict
    (without the "error" key, which only the cargo-metadata fetch can set).
    """
    pkg_to_dir = package_directories(meta, knope_packages)
    reverse = build_reverse_dep_graph(meta, knope_packages)

    direct_affected = match_changed_packages(changed_files, pkg_to_dir)

    # Expand with downstream dependents
    all_affected = set(direct_affected)
    downstream_only = set()
    for pkg in direct_affected:
        for dep in transitive_downstream(pkg, reverse):
            all_affected.add(dep)
            if dep not in direct_affected:
                downstream_only.add(dep)

    missing = sorted(all_affected - set(present.keys()))

    return {
        "error": "",
        "direct": sorted(direct_affected),
        "downstream": sorted(downstream_only),
        "required": sorted(all_affected),
        "present": sorted(present.items()),
        "missing": missing,
        "changeset_content": build_changeset_content(missing),
    }


def load_cargo_metadata():
    """Fetch workspace metadata (--no-deps avoids network access)."""
    return json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        text=True, stderr=subprocess.DEVNULL,
    ))


def main():
    changed_files = read_lines("CHANGED_FILES")
    changeset_files = read_lines("CHANGESET_FILES")

    knope_packages = load_knope_packages()
    present = parse_present_bumps(changeset_files, knope_packages)

    try:
        meta = load_cargo_metadata()
    except Exception as e:  # noqa: BLE001 - report any failure back to the workflow
        emit({
            "error": str(e),
            "direct": [], "downstream": [], "required": [],
            "present": sorted(present.items()), "missing": [],
            "changeset_content": "",
        })

    emit(detect(meta, knope_packages, changed_files, present))


if __name__ == "__main__":
    main()
