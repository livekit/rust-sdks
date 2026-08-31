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
# Make sure that every publishable crate in the workspace exists on crates.io.
# Trusted publishing cannot create a new crate. A new crate needs one manual
# publish before the release runs.
set -uo pipefail

CRATES=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.publish != []) | .name' | sort) || CRATES=""
if [ -z "$CRATES" ]; then
  echo "::error::Could not get the publishable crates from cargo metadata."
  exit 1
fi

MISSING=()
for crate in $CRATES; do
  CODE=$(curl -sS --retry 2 -o /dev/null -w '%{http_code}' \
    -A "livekit-rust-sdks-ci (github.com/livekit/rust-sdks)" \
    "https://crates.io/api/v1/crates/${crate}")
  case "$CODE" in
    200) ;;
    404) MISSING+=("$crate") ;;
    *) echo "::error::crates.io returned HTTP ${CODE:-none} for '${crate}'."; exit 1 ;;
  esac
done

if [ ${#MISSING[@]} -ne 0 ]; then
  echo "::error::These crates are not on crates.io and need one manual publish before the release: ${MISSING[*]}"
  exit 1
fi

echo "Every publishable crate exists on crates.io."
