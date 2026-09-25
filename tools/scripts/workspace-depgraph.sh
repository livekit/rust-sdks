#!/usr/bin/env bash
set -euo pipefail

# Generates a visualization of dependencies in this workspace.

# Install dependencies:
# brew install cargo-depgraph graphviz

REPO_ROOT="$(dirname "$(cargo locate-project --workspace --message-format plain)")"

# Get comma separated list of example names to exclude from graph.
# This is necessary since examples are currently workspace members.
shopt -s nullglob
examples=("${REPO_ROOT}/examples"/*)
examples=("${examples[@]##*/}")
IFS=","
excludes="${examples[*]:-}"
unset IFS

cd "${REPO_ROOT}"

cargo depgraph \
  --workspace-only \
  --all-features \
  --target-deps \
  --build-deps \
  --exclude "${excludes}" \
  | dot -Kdot -Tsvg -Grankdir=LR -Nfontname=Helvetica -Nstyle='rounded' -Nmargin='0.25,0' \
  -o "workspace-deps.svg"
