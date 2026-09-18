#!/usr/bin/env python3
# Copyright 2025 LiveKit, Inc.
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

"""Resolve cargo build flags for a downstream platform build of livekit-ffi.

`livekit-ffi` exposes two mutually exclusive FFI surfaces, `room-apis` and
`core-modules`. Which one each downstream package build gets is declared once,
in `[package.metadata.platform-features]` in livekit-ffi/Cargo.toml, and read
back here so a platform cannot silently drift onto the wrong surface.

    $ platform_features.py swift
    --no-default-features --features core-modules

    $ platform_features.py --surface core-modules
    --no-default-features --features core-modules

`--surface` is for build sites that are not platform builds and so have no
platform to name: the per-commit compile/test gates in builds.yml and
tests.yml, and the shared host cdylib in livekit-ffi/Makefile.toml that the
bindgens read UniFFI metadata from. Anything that builds an artifact for a
platform names that platform instead.

    $ platform_features.py --list
    android        --no-default-features --features core-modules
    ...

Used by the uniffi-* workflows and by livekit-ffi/Makefile.toml.
"""

import argparse
import json
import subprocess
import sys

PACKAGE = "livekit-ffi"
METADATA_KEY = "platform-features"


def load_table():
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    meta = json.loads(out)
    for pkg in meta["packages"]:
        if pkg["name"] == PACKAGE:
            table = (pkg.get("metadata") or {}).get(METADATA_KEY)
            if not table:
                sys.exit(
                    f"{PACKAGE} declares no [package.metadata.{METADATA_KEY}] table"
                )
            return table
    sys.exit(f"{PACKAGE} not found in cargo metadata")


def flags_for(entry, platform):
    features = entry.get("features") or []
    if not features:
        sys.exit(f"platform '{platform}' declares no features")

    surfaces = {f for f in features if f in ("room-apis", "core-modules")}
    if len(surfaces) != 1:
        sys.exit(
            f"platform '{platform}' must select exactly one of room-apis / "
            f"core-modules (got: {sorted(surfaces) or 'none'})"
        )
    # Every entry states its whole feature set. Without this, an entry inherits
    # whatever `default` happens to list, so a change there moves platforms
    # silently -- and for a core-modules entry it would select both surfaces,
    # which do not compile together.
    if not entry.get("no-default-features"):
        sys.exit(
            f"platform '{platform}' does not set no-default-features; every "
            f"entry must list the whole feature set it wants, so that a change "
            f"to the crate's `default` features cannot move it onto another "
            f"surface"
        )

    flags = []
    if entry.get("no-default-features"):
        flags.append("--no-default-features")
    flags += ["--features", ",".join(features)]
    return " ".join(flags)


def flags_for_surface(table, surface):
    """Flags shared by every platform on `surface`.

    For callers with no platform to name -- the per-commit gates in builds.yml
    and tests.yml, and the shared host build in livekit-ffi/Makefile.toml that
    serves the swift, android and dart flows at once. A caller that builds an
    artifact for one platform should name that platform instead.

    Every platform on the surface must agree, otherwise there is no single
    answer and the caller has to name a platform. That holds for core-modules
    and deliberately does not for room-apis, where the TLS backend varies by
    target.
    """
    resolved = {
        name: flags_for(entry, name)
        for name, entry in table.items()
        if surface in (entry.get("features") or [])
    }
    if not resolved:
        sys.exit(f"no platform declares the '{surface}' surface")
    distinct = set(resolved.values())
    if len(distinct) != 1:
        detail = "\n".join(f"  {n}: {f}" for n, f in sorted(resolved.items()))
        sys.exit(
            f"platforms on the '{surface}' surface do not share one feature "
            f"set, so it cannot be resolved by surface:\n{detail}"
        )
    return distinct.pop()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("platform", nargs="?", help="platform key to resolve")
    ap.add_argument(
        "--surface",
        choices=["room-apis", "core-modules"],
        help="flags shared by every platform on this surface; for build "
        "sites that are not platform builds (see the module docstring)",
    )
    ap.add_argument(
        "--list", action="store_true", help="print every platform and its flags"
    )
    args = ap.parse_args()

    table = load_table()

    if args.surface:
        print(flags_for_surface(table, args.surface))
        return

    if args.list:
        width = max(len(k) for k in table)
        for name in sorted(table):
            print(f"{name:<{width}}  {flags_for(table[name], name)}")
        return

    if not args.platform:
        ap.error("a platform key is required (or --surface / --list)")

    if args.platform not in table:
        sys.exit(
            f"unknown platform '{args.platform}'. Known: {', '.join(sorted(table))}"
        )
    print(flags_for(table[args.platform], args.platform))


if __name__ == "__main__":
    main()
