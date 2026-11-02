#!/usr/bin/env python3
"""
Taken from: https://github.com/webrtc-sdk/android/blob/main/tools/downgrade_class_version.py
Rewrite the class file version stamp of every .class in a jar (or an aar's classes.jar).

Upstream WebRTC started compiling the Android java sources with a JDK 21 target
(class file major version 65) as of M150. Nothing in those classes actually uses a
post-Java-17 class file feature, but the version stamp alone is enough to break
tools pinned to an older ASM -- notably the shadow plugin used to relocate
org.webrtc -> livekit.org.webrtc -- and D8 in older AGP versions used by consumers.

Stamping the classes back down to the version the previous release shipped (61 /
Java 17) keeps the artifacts byte-compatible with the M144 release.

Usage: downgrade_class_version.py [--major N] <file.jar|file.aar> [...]
"""

import argparse
import os
import shutil
import sys
import tempfile
import zipfile

DEFAULT_MAJOR = 61  # Java 17, matching the M144 release.
CAFEBABE = b"\xca\xfe\xba\xbe"


def rewrite_class(data, major):
    """Return (new_bytes, changed) for a single class file."""
    if len(data) < 8 or data[:4] != CAFEBABE:
        return data, False
    current = int.from_bytes(data[6:8], "big")
    if current <= major:
        return data, False
    return data[:6] + major.to_bytes(2, "big") + data[8:], True


def rewrite_zip(path, major, nested_names=()):
    """Rewrite class files in the zip at `path` in place.

    Entries listed in `nested_names` are themselves zips and get rewritten
    recursively (used for classes.jar inside an aar).
    """
    changed = 0
    with zipfile.ZipFile(path) as src:
        infos = src.infolist()
        entries = []
        for info in infos:
            data = src.read(info.filename)
            if info.filename.endswith(".class"):
                data, did = rewrite_class(data, major)
                changed += did
            elif info.filename in nested_names:
                data, did = rewrite_nested_zip(data, major)
                changed += did
            entries.append((info, data))

    tmp_fd, tmp_path = tempfile.mkstemp(dir=os.path.dirname(os.path.abspath(path)))
    os.close(tmp_fd)
    try:
        with zipfile.ZipFile(tmp_path, "w", zipfile.ZIP_DEFLATED) as out:
            for info, data in entries:
                new_info = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                new_info.compress_type = info.compress_type
                new_info.external_attr = info.external_attr
                out.writestr(new_info, data)
        shutil.move(tmp_path, path)
    except BaseException:
        if os.path.exists(tmp_path):
            os.remove(tmp_path)
        raise
    return changed


def rewrite_nested_zip(data, major):
    """Rewrite a zip held in memory. Returns (new_bytes, count_changed)."""
    with tempfile.NamedTemporaryFile(suffix=".jar", delete=False) as tmp:
        tmp.write(data)
        tmp_path = tmp.name
    try:
        changed = rewrite_zip(tmp_path, major)
        with open(tmp_path, "rb") as f:
            return f.read(), changed
    finally:
        os.remove(tmp_path)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--major", type=int, default=DEFAULT_MAJOR,
                        help="target class file major version (default: %(default)s)")
    parser.add_argument("files", nargs="+", help="jar or aar files to rewrite in place")
    args = parser.parse_args(argv)

    for path in args.files:
        if not os.path.isfile(path):
            print("error: no such file: %s" % path, file=sys.stderr)
            return 1
        nested = ("classes.jar",) if path.endswith(".aar") else ()
        changed = rewrite_zip(path, args.major, nested_names=nested)
        print("%s: stamped %d class file(s) down to major version %d"
              % (path, changed, args.major))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
