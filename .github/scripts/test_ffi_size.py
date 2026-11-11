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

"""Tests for ffi_size.py.

Run with:  python3 -m unittest discover -s .github/scripts -p 'test_*.py'
"""

import json
import os
import tempfile
import unittest

import ffi_size


class ArtifactPathTest(unittest.TestCase):
    def test_names_per_target_and_crate_type(self):
        cases = {
            ("x86_64-pc-windows-msvc", "cdylib"): "livekit_ffi.dll",
            ("aarch64-apple-darwin", "cdylib"): "liblivekit_ffi.dylib",
            ("aarch64-apple-ios", "staticlib"): "liblivekit_ffi.a",
            ("x86_64-unknown-linux-gnu", "cdylib"): "liblivekit_ffi.so",
            ("aarch64-linux-android", "cdylib"): "liblivekit_ffi.so",
        }
        for (target, crate_type), name in cases.items():
            path = ffi_size.artifact_path("target", target, "release", crate_type)
            self.assertEqual(path, os.path.join("target", target, "release", name))


class CrateTypeTest(unittest.TestCase):
    def test_reads_crate_type(self):
        flags = "--lib --crate-type staticlib --no-default-features --features core-modules"
        self.assertEqual(ffi_size.crate_type_from_flags(flags), "staticlib")

    def test_missing_crate_type_fails(self):
        with self.assertRaises(SystemExit):
            ffi_size.crate_type_from_flags("--no-default-features --features core-modules")


class MeasureAndSummaryTest(unittest.TestCase):
    def test_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            target_dir = os.path.join(tmp, "target")
            lib = ffi_size.artifact_path(target_dir, "aarch64-linux-android", "release", "cdylib")
            os.makedirs(os.path.dirname(lib))
            with open(lib, "wb") as f:
                f.write(b"\0" * 4096)

            sizes = os.path.join(tmp, "sizes")
            ffi_size.main([
                "measure", "--row", "room-apis", "--target", "aarch64-linux-android",
                "--flags", "--lib --crate-type cdylib --features room-apis",
                "--target-dir", target_dir,
                "--out", os.path.join(sizes, "nested", "a.json"),
            ])

            merged = os.path.join(tmp, "summary.json")
            md = os.path.join(tmp, "summary.md")
            ffi_size.main(["summary", sizes, "--sha", "0123456789", "--json", merged, "--markdown", md])

            with open(merged) as f:
                data = json.load(f)
            self.assertEqual(data["sha"], "0123456789")
            self.assertEqual(len(data["rows"]), 1)
            row = data["rows"][0]
            self.assertEqual(row["bytes"], 4096)
            self.assertEqual(row["crate_type"], "cdylib")
            self.assertLess(row["gzip_bytes"], 4096)

            with open(md) as f:
                text = f.read()
            self.assertIn("`0123456`", text)
            self.assertIn("| room-apis | `aarch64-linux-android` | cdylib | 4.00 KiB |", text)

    def test_missing_artifact_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(SystemExit):
                ffi_size.main([
                    "measure", "--row", "x", "--target", "aarch64-apple-ios",
                    "--flags", "--crate-type staticlib", "--target-dir", tmp,
                    "--out", os.path.join(tmp, "o.json"),
                ])

    def test_empty_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            md = os.path.join(tmp, "summary.md")
            ffi_size.main(["summary", tmp, "--markdown", md])
            with open(md) as f:
                self.assertIn("no builds were measured", f.read())


if __name__ == "__main__":
    unittest.main()
