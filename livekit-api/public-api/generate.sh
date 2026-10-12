#!/bin/bash
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

# Regenerates ../src/public_api/generated.rs from openapi.yaml with progenitor (see generate/).
#
#   ./generate.sh                         regenerate from the vendored openapi.yaml
#   ./generate.sh <public-api-server> [ref]  vendor pkg/api/v1/openapi.yaml at ref
#                                         (default origin/main) first

set -euo pipefail
cd "$(dirname "$0")"

if [ $# -ge 1 ]; then
    SERVER=$1
    REF=${2:-origin/main}
    SHA=$(git -C "$SERVER" rev-parse "$REF")
    {
        echo "# Vendored from livekit/public-api-server@$SHA:pkg/api/v1/openapi.yaml"
        echo "# Regenerate the client with generate.sh; do not edit by hand."
        git -C "$SERVER" show "$SHA:pkg/api/v1/openapi.yaml"
    } > openapi.yaml
fi

OUT=../src/public_api/generated.rs
cargo run --quiet --manifest-path generate/Cargo.toml --target-dir ../../target -- openapi.yaml "$OUT"
rustfmt --edition 2021 "$OUT"
