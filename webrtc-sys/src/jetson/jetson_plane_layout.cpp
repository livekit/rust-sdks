/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "jetson_plane_layout.h"

namespace livekit {

int ResolvePlaneStride(const PlaneLayoutHints& hints,
                       int expected_height,
                       int min_stride,
                       int fallback) {
  // 1) NvBufferPlane::fmt.stride if present and sane.
  if (hints.fmt_stride >= min_stride) {
    return hints.fmt_stride;
  }

  // 2) NvBufSurfaceFromFd pitch (ground truth for this plane fd).
  if (hints.probed_pitch >= min_stride) {
    return hints.probed_pitch;
  }

  // 3) Derive from the mapped plane length. fmt.height can be misleading (e.g.
  // a UV plane reporting full luma height), which would yield an under-stride
  // (pitch/2) and a green image. Only accept a derived value that meets
  // min_stride; prefer fmt.height as the divisor, then expected_height.
  int best = 0;
  if (hints.plane_length > 0) {
    if (hints.fmt_height > 0) {
      const int derived = hints.plane_length / hints.fmt_height;
      if (derived >= min_stride) {
        best = derived;
      }
    }
    if (best == 0 && expected_height > 0) {
      const int derived = hints.plane_length / expected_height;
      if (derived >= min_stride) {
        best = derived;
      }
    }
  }

  int stride = best > 0 ? best : fallback;
  if (stride < min_stride) {
    stride = min_stride;
  }

  // Cap stride to what the allocation can accommodate so we never walk past the
  // mapped plane. (This should not normally trigger.)
  if (expected_height > 0 && hints.plane_length > 0) {
    const int max_stride = hints.plane_length / expected_height;
    if (max_stride > 0 && stride > max_stride) {
      stride = max_stride;
    }
  }

  return stride;
}

int ResolvePlaneHeight(bool have_probe, int probed_height, int expected_height) {
  if (have_probe && probed_height >= expected_height) {
    return probed_height;
  }
  return expected_height;
}

}  // namespace livekit
