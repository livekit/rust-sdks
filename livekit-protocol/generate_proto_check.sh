#!/bin/bash
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

# MANUAL CHECK: no CI workflow runs this script. Run it by hand after a change to
# `generate_proto.sh`, to the protocol submodule or to a generator version.
#
# Checks `generate_proto.sh` in a scratch copy, never in `src/`:
#   1. a protoc that fails (exit 47) fails the script with 47 and leaves stale outputs untouched;
#   2. the same with an empty output directory;
#   3. a failing normalizer (perl exit 3) fails the script with 3;
#   4. the real generators reproduce the committed `src/` byte for byte, with no trailing
#      whitespace, and a second run changes nothing.
# Run from this directory with the generator dependencies installed (see generate_proto.sh).
set -u
cd "$(dirname "$0")"
HERE=$PWD
# Fail closed if no scratch directory could be created: nothing below may touch a path that is
# not an owned, freshly created temporary directory.
SCRATCH=$(mktemp -d) || { echo "generate_proto_check: FAIL: mktemp -d failed" >&2; exit 2; }
[ -n "$SCRATCH" ] && [ -d "$SCRATCH" ] || { echo "generate_proto_check: FAIL: no scratch directory" >&2; exit 2; }
trap 'rm -rf "${SCRATCH:?}"' EXIT
GENERATED="livekit.rs livekit.serde.rs livekit.agent.rs livekit.agent.serde.rs logger.rs logger.serde.rs"
fail() { echo "generate_proto_check: FAIL: $*" >&2; exit 1; }

# A scratch generation directory with the script and a chosen set of outputs.
scratch() {
    rm -rf "${SCRATCH:?}/gen"; mkdir -p "$SCRATCH/gen/src"
    cp generate_proto.sh "$SCRATCH/gen/"
}
# A fake tool first on PATH that exits with the given status.
fake() {
    mkdir -p "$SCRATCH/bin"; printf '#!/bin/sh\nexit %s\n' "$2" > "$SCRATCH/bin/$1"; chmod +x "$SCRATCH/bin/$1"
}
run_gen() { (cd "$SCRATCH/gen" && PATH="$SCRATCH/bin:$PATH" PROTOCOL="$HERE/protocol/protobufs" ./generate_proto.sh > "$SCRATCH/out.log" 2>&1); echo $?; }

# 1. Failing protoc, stale outputs: status 47, stale bytes untouched (not normalized).
scratch; fake protoc 47
for f in $GENERATED; do printf 'stale = \n' > "$SCRATCH/gen/src/$f"; done
[ "$(run_gen)" = 47 ] || fail "failing protoc with stale outputs did not return 47"
for f in $GENERATED; do [ "$(cat "$SCRATCH/gen/src/$f")" = 'stale = ' ] || fail "stale $f was changed after a failed protoc"; done
rm -f "$SCRATCH/bin/protoc"

# 2. Failing protoc, empty output directory: status 47.
scratch; fake protoc 47
[ "$(run_gen)" = 47 ] || fail "failing protoc with an empty output directory did not return 47"
rm -f "$SCRATCH/bin/protoc"

# 3. Failing normalizer: status 3.
scratch; fake perl 3
[ "$(run_gen)" = 3 ] || fail "a failing normalizer did not fail the script with 3"
rm -f "$SCRATCH/bin/perl"

# 4. Real generators: byte-identical to src/, no trailing whitespace, second run identical.
scratch
[ "$(run_gen)" = 0 ] || { cat "$SCRATCH/out.log" >&2; fail "generation failed"; }
for f in $GENERATED; do
    cmp -s "$SCRATCH/gen/src/$f" "src/$f" || fail "$f differs from the committed source"
    grep -q '[[:space:]]$' "$SCRATCH/gen/src/$f" && fail "$f has trailing whitespace"
done
cp -R "$SCRATCH/gen/src" "$SCRATCH/first"
[ "$(run_gen)" = 0 ] || fail "second generation failed"
for f in $GENERATED; do cmp -s "$SCRATCH/gen/src/$f" "$SCRATCH/first/$f" || fail "$f changed on the second run"; done
echo "generate_proto_check: OK (failed protoc 47 twice, failed normalizer 3, generation reproducible and whitespace-clean)"
