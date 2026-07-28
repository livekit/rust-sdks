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

"""Tests for changeset_detect.py.

Run with:  python3 -m unittest discover -s .github/scripts -p 'test_*.py'
       or:  python3 .github/scripts/test_changeset_detect.py

Uses only the standard library and a synthetic workspace, so it never invokes
cargo or reads the real workspace.
"""

import os
import tempfile
import unittest

import changeset_detect as cd


def make_meta(packages, deps, workspace_root="/ws"):
    """Build a fake `cargo metadata` document.

    packages: {name -> relative dir}
    deps:     {name -> [dependency names]}
    """
    return {
        "workspace_root": workspace_root,
        "packages": [
            {
                "name": name,
                "manifest_path": f"{workspace_root}/{rel}/Cargo.toml",
                "dependencies": [{"name": d} for d in deps.get(name, [])],
            }
            for name, rel in packages.items()
        ],
    }


# A small synthetic workspace:
#   a-sys  (leaf)
#   b      -> depends on a-sys
#   c      -> depends on b        (so c transitively depends on a-sys)
#   d      -> independent
PACKAGES = {"a-sys": "a-sys", "b": "b", "c": "c", "d": "d"}
DEPS = {"b": ["a-sys"], "c": ["b"], "d": []}
KNOPE = set(PACKAGES)
META = make_meta(PACKAGES, DEPS)


class TestGraph(unittest.TestCase):
    def test_reverse_dep_graph(self):
        reverse = cd.build_reverse_dep_graph(META, KNOPE)
        self.assertEqual(reverse["a-sys"], {"b"})
        self.assertEqual(reverse["b"], {"c"})
        self.assertEqual(reverse["c"], set())
        self.assertEqual(reverse["d"], set())

    def test_reverse_dep_graph_ignores_non_knope_deps(self):
        meta = make_meta({"b": "b"}, {"b": ["serde", "tokio"]})
        reverse = cd.build_reverse_dep_graph(meta, {"b"})
        self.assertEqual(reverse["b"], set())

    def test_transitive_downstream(self):
        reverse = cd.build_reverse_dep_graph(META, KNOPE)
        self.assertEqual(cd.transitive_downstream("a-sys", reverse), {"b", "c"})
        self.assertEqual(cd.transitive_downstream("b", reverse), {"c"})
        self.assertEqual(cd.transitive_downstream("c", reverse), set())
        self.assertEqual(cd.transitive_downstream("d", reverse), set())

    def test_transitive_downstream_handles_cycle(self):
        # x <-> y depend on each other; closure must terminate
        meta = make_meta({"x": "x", "y": "y"}, {"x": ["y"], "y": ["x"]})
        reverse = cd.build_reverse_dep_graph(meta, {"x", "y"})
        self.assertEqual(cd.transitive_downstream("x", reverse), {"x", "y"})


class TestMatchChangedPackages(unittest.TestCase):
    def setUp(self):
        self.pkg_to_dir = cd.package_directories(META, KNOPE)

    def test_matches_by_directory_prefix(self):
        self.assertEqual(
            cd.match_changed_packages(["b/src/lib.rs"], self.pkg_to_dir), {"b"}
        )

    def test_no_match_for_unversioned_paths(self):
        self.assertEqual(
            cd.match_changed_packages(["README.md", "docs/x.md"], self.pkg_to_dir), set()
        )

    def test_multiple_packages(self):
        self.assertEqual(
            cd.match_changed_packages(["a-sys/x.rs", "d/y.rs"], self.pkg_to_dir),
            {"a-sys", "d"},
        )

    def test_longest_prefix_wins_for_nested_packages(self):
        # A nested package must win over its parent-directory package.
        meta = make_meta({"outer": "pkg", "inner": "pkg/inner"}, {})
        pkg_to_dir = cd.package_directories(meta, {"outer", "inner"})
        self.assertEqual(
            cd.match_changed_packages(["pkg/inner/src/lib.rs"], pkg_to_dir), {"inner"}
        )
        self.assertEqual(
            cd.match_changed_packages(["pkg/src/lib.rs"], pkg_to_dir), {"outer"}
        )

    def test_prefix_requires_directory_boundary(self):
        # "a-sys-extra/x" must not match package dir "a-sys".
        meta = make_meta({"a-sys": "a-sys"}, {})
        pkg_to_dir = cd.package_directories(meta, {"a-sys"})
        self.assertEqual(
            cd.match_changed_packages(["a-sys-extra/x.rs"], pkg_to_dir), set()
        )


class TestParsePresentBumps(unittest.TestCase):
    def _write(self, tmp, name, body):
        path = os.path.join(tmp, name)
        with open(path, "w") as f:
            f.write(body)
        return path

    def test_parses_unquoted_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = self._write(tmp, "a.md", "---\na-sys: patch\nb: minor\n---\n\ndesc\n")
            self.assertEqual(
                cd.parse_present_bumps([p], KNOPE), ({"a-sys": "patch", "b": "minor"}, [])
            )

    def test_quoted_names_flagged_invalid(self):
        # Knope silently ignores the quoted JS-changesets format; it must be
        # reported as invalid rather than counted as coverage.
        with tempfile.TemporaryDirectory() as tmp:
            p = self._write(tmp, "a.md", '---\n"a-sys": patch\nb: minor\n---\n\ndesc\n')
            present, invalid = cd.parse_present_bumps([p], KNOPE)
            self.assertEqual(present, {"b": "minor"})
            self.assertEqual(invalid, [{
                "file": p,
                "line": 2,
                "content": '"a-sys": patch',
                "fixed": "a-sys: patch",
            }])

    def test_ignores_unknown_packages(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = self._write(tmp, "a.md", "---\nnot-a-package: patch\nb: patch\n---\n\nd\n")
            self.assertEqual(cd.parse_present_bumps([p], KNOPE), ({"b": "patch"}, []))

    def test_highest_bump_wins_across_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            p1 = self._write(tmp, "a.md", "---\nb: patch\n---\n\nd\n")
            p2 = self._write(tmp, "z.md", "---\nb: major\n---\n\nd\n")
            self.assertEqual(cd.parse_present_bumps([p1, p2], KNOPE), ({"b": "major"}, []))
            # order independent
            self.assertEqual(cd.parse_present_bumps([p2, p1], KNOPE), ({"b": "major"}, []))

    def test_ignores_invalid_bump_values(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = self._write(tmp, "a.md", "---\nb: bogus\n---\n\nd\n")
            self.assertEqual(cd.parse_present_bumps([p], KNOPE), ({}, []))

    def test_missing_front_matter(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = self._write(tmp, "a.md", "just a description, no front matter\n")
            self.assertEqual(cd.parse_present_bumps([p], KNOPE), ({}, []))

    def test_missing_file_is_skipped(self):
        self.assertEqual(cd.parse_present_bumps(["/nonexistent/x.md"], KNOPE), ({}, []))


class TestDetect(unittest.TestCase):
    def test_no_changeset_flags_full_closure(self):
        r = cd.detect(META, KNOPE, ["a-sys/src/lib.rs"], {})
        self.assertEqual(r["direct"], ["a-sys"])
        self.assertEqual(r["downstream"], ["b", "c"])
        self.assertEqual(r["required"], ["a-sys", "b", "c"])
        self.assertEqual(r["missing"], ["a-sys", "b", "c"])
        self.assertEqual(r["error"], "")

    def test_incomplete_changeset_flags_downstream(self):
        r = cd.detect(META, KNOPE, ["a-sys/src/lib.rs"], {"a-sys": "patch"})
        self.assertEqual(r["required"], ["a-sys", "b", "c"])
        self.assertEqual(r["present"], [("a-sys", "patch")])
        self.assertEqual(r["missing"], ["b", "c"])

    def test_complete_changeset_has_no_missing(self):
        present = {"a-sys": "patch", "b": "patch", "c": "minor"}
        r = cd.detect(META, KNOPE, ["a-sys/src/lib.rs"], present)
        self.assertEqual(r["missing"], [])

    def test_unversioned_change_requires_nothing(self):
        r = cd.detect(META, KNOPE, ["README.md", ".github/x.yml"], {})
        self.assertEqual(r["required"], [])
        self.assertEqual(r["missing"], [])

    def test_extra_bumps_do_not_cause_missing(self):
        # Changeset bumps more than required — still complete.
        present = {"a-sys": "patch", "b": "patch", "c": "patch", "d": "patch"}
        r = cd.detect(META, KNOPE, ["a-sys/src/lib.rs"], present)
        self.assertEqual(r["missing"], [])

    def test_changeset_content_prefills_missing(self):
        r = cd.detect(META, KNOPE, ["a-sys/src/lib.rs"], {"a-sys": "patch"})
        # missing == [b, c]; the prefilled changeset must bump exactly those
        self.assertIn("b: patch", r["changeset_content"])
        self.assertIn("c: patch", r["changeset_content"])
        self.assertNotIn("a-sys", r["changeset_content"])


class TestBuildChangesetContent(unittest.TestCase):
    def test_deterministic_with_explicit_metadata(self):
        content = cd.build_changeset_content(
            ["b", "c"], pr_title="My change", pr_number="42", pr_author="alice"
        )
        self.assertEqual(
            content,
            "---\nb: patch\nc: patch\n---\n\nMy change - #42 (@alice)",
        )

    def test_empty_missing_produces_empty_front_matter(self):
        content = cd.build_changeset_content([], pr_title="t", pr_number="1", pr_author="a")
        self.assertEqual(content, "---\n---\n\nt - #1 (@a)")


if __name__ == "__main__":
    unittest.main()
