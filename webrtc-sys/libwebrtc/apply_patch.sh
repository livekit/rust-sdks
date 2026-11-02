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

# Sourced by the build scripts. Do not run directly.

# Usage: apply_patch <patch> [dir]
#
# Applies <patch> to the git checkout at [dir] (default: current directory).
# A patch that is already applied is skipped, so a build can be rerun on the
# same checkout. A patch that neither applies nor is already applied stops the
# build. Carrying on would produce a libwebrtc that silently lacks the change.
apply_patch() {
  local patch="$1"
  local dir="${2:-.}"
  local name
  name="$(basename "$patch")"
  local flags=(--ignore-space-change --ignore-whitespace --whitespace=nowarn)

  if git -C "$dir" apply --reverse --check "${flags[@]}" "$patch" 2>/dev/null; then
    echo "Patch already applied: $name"
    return 0
  fi

  echo "Applying patch: $name"
  # git apply is all or nothing, so a failure leaves the checkout untouched.
  if ! git -C "$dir" apply -v "${flags[@]}" "$patch"; then
    echo "Error: $name does not apply to $(cd "$dir" && pwd)." >&2
    echo "Regenerate the patch against the pinned WebRTC revision, or reset a" >&2
    echo "partially patched checkout (git checkout -- .) and rerun." >&2
    exit 1
  fi
}
