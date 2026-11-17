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

    def test_pull_request_names_both_commits(self):
        with tempfile.TemporaryDirectory() as tmp:
            merged = os.path.join(tmp, "summary.json")
            md = os.path.join(tmp, "summary.md")
            ffi_size.main([
                "summary", tmp, "--sha", "aaaaaaaaaa", "--pr-head-sha", "bbbbbbbbbb",
                "--json", merged, "--markdown", md,
            ])

            with open(merged) as f:
                data = json.load(f)
            self.assertEqual(data["sha"], "aaaaaaaaaa")
            self.assertEqual(data["pr_head_sha"], "bbbbbbbbbb")

            with open(md) as f:
                text = f.read()
            self.assertIn("Merge commit `aaaaaaa` of PR head `bbbbbbb`", text)

    def test_push_omits_pr_head(self):
        with tempfile.TemporaryDirectory() as tmp:
            merged = os.path.join(tmp, "summary.json")
            ffi_size.main(["summary", tmp, "--sha", "aaaaaaaaaa", "--pr-head-sha", "",
                           "--json", merged])
            with open(merged) as f:
                self.assertNotIn("pr_head_sha", json.load(f))

    def test_empty_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            md = os.path.join(tmp, "summary.md")
            ffi_size.main(["summary", tmp, "--markdown", md])
            with open(md) as f:
                self.assertIn("no builds were measured", f.read())

    def test_merged_summary_in_input_is_ignored(self):
        with tempfile.TemporaryDirectory() as tmp:
            sizes = os.path.join(tmp, "sizes")
            os.makedirs(sizes)
            row = {"row": "room-apis", "target": "aarch64-linux-android", "crate_type": "cdylib",
                   "bytes": 10, "gzip_bytes": 5}
            with open(os.path.join(sizes, "room-apis-aarch64-linux-android.json"), "w") as f:
                json.dump(row, f)
            # What an earlier attempt of the report job uploaded.
            with open(os.path.join(sizes, "size-summary.json"), "w") as f:
                json.dump({"sha": "0123456789", "rows": [row]}, f)

            self.assertEqual(ffi_size.load_rows(sizes), [row])


def row(name, target, size):
    return {"row": name, "target": target, "crate_type": "cdylib", "bytes": size, "gzip_bytes": size // 2}


class FindBaselineTest(unittest.TestCase):
    def fake_api(self, measured):
        """main is c3 <- c2 <- c1 (oldest); `measured` maps a sha to whether
        its run kept the summary artifact."""
        def api(path):
            if path.startswith("repos/o/r/compare/main...head"):
                return {"merge_base_commit": {"sha": "c3"}}
            if path.startswith("repos/o/r/commits?sha=c3"):
                return [{"sha": "c3"}, {"sha": "c2"}, {"sha": "c1"}]
            if "/actions/workflows/ci.yml/runs?head_sha=" in path:
                sha = path.split("head_sha=")[1].split("&")[0]
                return {"workflow_runs": [{"id": int(sha[1:])}] if sha in measured else []}
            if "/artifacts?name=ffi-size-summary" in path:
                sha = "c" + path.split("/runs/")[1].split("/")[0]
                return {"artifacts": [{"expired": not measured[sha]}]}
            raise AssertionError(path)
        return api

    def test_merge_base_measured(self):
        found = ffi_size.find_baseline(self.fake_api({"c3": True}), "o/r", "ci.yml", "main", "head")
        self.assertEqual((found["sha"], found["walked"], found["run_id"]), ("c3", 0, 3))

    def test_walks_back_past_unmeasured_and_expired(self):
        api = self.fake_api({"c3": False, "c1": True})
        found = ffi_size.find_baseline(api, "o/r", "ci.yml", "main", "head")
        self.assertEqual((found["sha"], found["walked"], found["merge_base"]), ("c1", 2, "c3"))

    def test_nothing_measured(self):
        self.assertIsNone(ffi_size.find_baseline(self.fake_api({}), "o/r", "ci.yml", "main", "head"))


class RenderComparisonTest(unittest.TestCase):
    found = {"run_id": 1, "sha": "b" * 40, "merge_base": "m" * 40, "walked": 0, "base_ref": "main"}

    def test_flags_growth_missing_and_new_rows(self):
        base = {"rows": [
            row("room-apis", "aarch64-linux-android", 1000000),
            row("room-apis", "x86_64-apple-darwin", 1000000),
            row("room-apis", "aarch64-apple-ios", 1000000),
        ]}
        head = {"sha": "a" * 40, "rows": [
            row("room-apis", "aarch64-linux-android", 1050000),
            row("room-apis", "x86_64-apple-darwin", 1000000),
            row("core-modules", "aarch64-linux-android", 500),
        ]}
        text = ffi_size.render_comparison(head, base, self.found)
        self.assertTrue(text.startswith(ffi_size.COMMENT_MARKER))
        self.assertIn("Largest change: room-apis `aarch64-linux-android` +48.83 KiB (+5.00%).", text)
        self.assertIn("| ⚠️ | room-apis | `aarch64-linux-android` |", text)
        self.assertIn("|  | room-apis | `x86_64-apple-darwin` | 976.56 KiB | 976.56 KiB | 0 B | +0.00% |", text)
        self.assertIn("| ❌ | room-apis | `aarch64-apple-ios` | 976.56 KiB | _not measured_ |", text)
        self.assertIn("| 🆕 | core-modules | `aarch64-linux-android` |", text)
        self.assertIn("Compared with `main` at `bbbbbbb`, the merge-base.", text)

    def test_shrink_is_not_flagged(self):
        base = {"rows": [row("room-apis", "t", 1000)]}
        head = {"sha": "a" * 40, "rows": [row("room-apis", "t", 900)]}
        text = ffi_size.render_comparison(head, base, self.found)
        self.assertIn("|  | room-apis | `t` | 1000 B | 900 B | -100 B | -10.00% |", text)

    def test_no_changes(self):
        rows = [row("room-apis", "t", 1000)]
        text = ffi_size.render_comparison({"sha": "a" * 40, "rows": rows}, {"rows": rows}, self.found)
        self.assertIn("No size changes.", text)

    def test_walked_back(self):
        found = dict(self.found, walked=3)
        rows = [row("room-apis", "t", 1000)]
        text = ffi_size.render_comparison({"sha": "a" * 40, "rows": rows}, {"rows": rows}, found)
        self.assertIn("the nearest measured ancestor of the merge-base `mmmmmmm` (3 commits back)", text)

    def test_no_baseline(self):
        head = {"sha": "a" * 40, "rows": [row("room-apis", "t", 1000)]}
        text = ffi_size.render_comparison(head, None, None)
        self.assertIn("nothing to compare against yet", text)
        self.assertIn("| 🆕 | room-apis | `t` |", text)

    def test_compare_command_reads_null_baseline(self):
        with tempfile.TemporaryDirectory() as tmp:
            head = os.path.join(tmp, "head.json")
            found = os.path.join(tmp, "baseline.json")
            md = os.path.join(tmp, "report.md")
            with open(head, "w") as f:
                json.dump({"sha": "a" * 40, "rows": [row("room-apis", "t", 1000)]}, f)
            with open(found, "w") as f:
                f.write("null\n")
            ffi_size.main(["compare", "--head", head, "--baseline", found,
                           "--base-summary", os.path.join(tmp, "missing.json"), "--markdown", md])
            with open(md) as f:
                self.assertIn("nothing to compare against yet", f.read())


if __name__ == "__main__":
    unittest.main()
