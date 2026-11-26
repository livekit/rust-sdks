#!/usr/bin/env bash
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
#
# The API command of `verwalter check` (`[check].api` in verwalter.toml).
# verwalter runs it in the path of a package, with VERWALTER_PACKAGE set. It
# prints the kind of change that the public API needs, `major` or `minor`, or
# nothing.
#
# The baseline is the base of the PR, not the last release tag. A changeset
# makes a claim for the changes of its own PR. Against the tag, a breaking
# change that is on `main` and not released would count against each later
# PR for the same crate.
# No `pipefail`: the tool exits with 100 for a finding and 101 for a build
# failure, and verwalter takes an exit code that is not 0 as a failed check.
set -eu

base=$(git merge-base HEAD "origin/${GITHUB_BASE_REF:-main}")

# One worktree of the base for all packages of a run. `--baseline-rev` is not
# enough: cargo-semver-checks copies the tree without its submodules, and
# yuv-sys does not build without libyuv.
baseline="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/api-baseline-${base}"
if [ ! -d "$baseline" ]; then
  # The baseline needs no LFS files.
  GIT_LFS_SKIP_SMUDGE=1 git worktree add --detach "$baseline" "$base" >&2
  git -C "$baseline" submodule update --init >&2
fi

# - DOCS_RS=1: webrtc-sys/build.rs then skips the native build.
# - --default-features: `livekit` and `livekit-api` have TLS features that do
#   not build together. An API behind another feature is not checked.
# - If the tool cannot build the documentation, it prints no summary line.
#   This command then prints nothing, and the check passes. The output of
#   the tool is on stderr, for the PR comment, and in the summary of the run.
DOCS_RS=1 cargo semver-checks --package "$VERWALTER_PACKAGE" --default-features --baseline-root "$baseline" 2>&1 |
  tee -a /dev/stderr "${GITHUB_STEP_SUMMARY:-/dev/null}" |
  sed -n 's/.*requires new \([a-z]*\) version.*/\1/p'
