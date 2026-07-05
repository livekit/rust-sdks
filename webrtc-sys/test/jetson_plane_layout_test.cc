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

// Standalone, hardware-free unit test for the Jetson plane-layout helpers.
// It depends on nothing but the C++ standard library, so it can be built and
// run on any host (no Jetson Multimedia API required):
//
//   c++ -std=c++17 -I ../src/jetson \
//       jetson_plane_layout_test.cc ../src/jetson/jetson_plane_layout.cpp \
//       -o jetson_plane_layout_test && ./jetson_plane_layout_test

#include "jetson_plane_layout.h"

#include <cstdio>
#include <cstdlib>

namespace {

int g_failures = 0;

void ExpectEq(int expected, int actual, const char* what) {
  if (expected != actual) {
    std::fprintf(stderr, "FAIL: %s: expected %d, got %d\n", what, expected,
                 actual);
    ++g_failures;
  }
}

using livekit::PlaneLayoutHints;
using livekit::ResolvePlaneHeight;
using livekit::ResolvePlaneStride;

void TestStridePrefersFmtStride() {
  PlaneLayoutHints h;
  h.fmt_stride = 1024;
  h.probed_pitch = 2048;  // Should be ignored when fmt_stride is sane.
  h.plane_length = 1024 * 720;
  h.fmt_height = 720;
  ExpectEq(1024, ResolvePlaneStride(h, 720, /*min*/ 640, /*fallback*/ 640),
           "prefers fmt_stride");
}

void TestStrideFallsBackToProbedPitch() {
  PlaneLayoutHints h;
  h.fmt_stride = 0;  // Unset in MMAP mode.
  h.probed_pitch = 768;
  h.plane_length = 768 * 720;
  h.fmt_height = 720;
  ExpectEq(768, ResolvePlaneStride(h, 720, /*min*/ 640, /*fallback*/ 640),
           "falls back to probed pitch");
}

void TestStrideDerivesFromLengthAndFmtHeight() {
  PlaneLayoutHints h;
  h.fmt_stride = 0;
  h.probed_pitch = 0;
  h.plane_length = 768 * 720;  // -> 768 per row
  h.fmt_height = 720;
  ExpectEq(768, ResolvePlaneStride(h, 720, /*min*/ 640, /*fallback*/ 640),
           "derives from length / fmt_height");
}

void TestStrideIgnoresMisleadingFmtHeight() {
  // UV plane reporting full luma height would derive pitch/2 (an under-stride),
  // which must be rejected in favour of expected_height.
  PlaneLayoutHints h;
  h.fmt_stride = 0;
  h.probed_pitch = 0;
  h.plane_length = 384 * 360;  // chroma plane: 384 stride * 360 rows
  h.fmt_height = 720;          // misleading: full luma height -> derives 192
  ExpectEq(384, ResolvePlaneStride(h, /*expected_height*/ 360, /*min*/ 320,
                                   /*fallback*/ 320),
           "ignores misleading fmt_height under-stride");
}

void TestStrideUsesFallbackWhenNothingReliable() {
  PlaneLayoutHints h;  // all zero
  ExpectEq(640, ResolvePlaneStride(h, 720, /*min*/ 640, /*fallback*/ 640),
           "uses fallback");
}

void TestStrideClampsUpToMinStride() {
  PlaneLayoutHints h;  // all zero; fallback below min
  ExpectEq(640, ResolvePlaneStride(h, 720, /*min*/ 640, /*fallback*/ 100),
           "clamps up to min_stride");
}

void TestStrideCapsDerivedToAllocation() {
  // A derived stride that exceeds what the allocation can hold (here because
  // fmt_height under-reports the true row count) is capped to
  // plane_length / expected_height. Note: the cap intentionally does not apply
  // to the fmt_stride / probed_pitch early-return paths.
  PlaneLayoutHints h;
  h.fmt_stride = 0;
  h.probed_pitch = 0;
  h.plane_length = 800 * 720;  // max_stride == 800 at expected_height 720
  h.fmt_height = 600;          // derives 960, which must be capped to 800
  ExpectEq(800, ResolvePlaneStride(h, /*expected_height*/ 720, /*min*/ 640,
                                   /*fallback*/ 640),
           "caps derived stride to allocation max_stride");
}

void TestHeightUsesProbeWhenValid() {
  ExpectEq(736, ResolvePlaneHeight(/*have_probe*/ true, 736, 720),
           "uses probed height when >= expected");
}

void TestHeightFallsBackWhenProbeMissingOrSmall() {
  ExpectEq(720, ResolvePlaneHeight(/*have_probe*/ false, 736, 720),
           "ignores probe when unavailable");
  ExpectEq(720, ResolvePlaneHeight(/*have_probe*/ true, 700, 720),
           "ignores probe smaller than expected");
}

}  // namespace

int main() {
  TestStridePrefersFmtStride();
  TestStrideFallsBackToProbedPitch();
  TestStrideDerivesFromLengthAndFmtHeight();
  TestStrideIgnoresMisleadingFmtHeight();
  TestStrideUsesFallbackWhenNothingReliable();
  TestStrideClampsUpToMinStride();
  TestStrideCapsDerivedToAllocation();
  TestHeightUsesProbeWhenValid();
  TestHeightFallsBackWhenProbeMissingOrSmall();

  if (g_failures != 0) {
    std::fprintf(stderr, "%d test(s) failed\n", g_failures);
    return EXIT_FAILURE;
  }
  std::printf("All jetson_plane_layout tests passed\n");
  return EXIT_SUCCESS;
}
