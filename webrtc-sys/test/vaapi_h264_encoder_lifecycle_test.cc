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

/*
 * Lifecycle test for the VA handles owned by the VAAPI H.264 encoder wrapper.
 * It checks that per-frame parameter buffers are destroyed by the end of each
 * Encode(), and that all buffers, surfaces, images, the context and the
 * config are destroyed before teardown terminates the display, after which
 * the process's open file descriptor count must be back to its baseline.
 * Buffer allocations and vaEndPicture() are made to fail to cover the error
 * paths. Host memory and objects the driver allocates internally are not
 * observed.
 *
 * Like jetson_plane_layout_test.cc, this test is built by hand. Unlike that
 * test, it requires VAAPI H.264 encoding support, which CI runners do not
 * have, and prints SKIP when it is unavailable. The linker's --wrap (GNU ld,
 * gold or lld) routes the wrapper's VA calls through the counting functions
 * below; test/shims stands in for the two rtc_base headers the wrapper
 * includes, so no libwebrtc build is needed. The "failed" errors in the
 * output are the injected failures. From webrtc-sys/:
 *
 *   c++ -std=c++20 -I test/shims -I src/vaapi \
 *       test/vaapi_h264_encoder_lifecycle_test.cc \
 *       src/vaapi/vaapi_h264_encoder_wrapper.cpp \
 *       src/vaapi/vaapi_display_drm.cpp \
 *       -Wl,--wrap=vaCreateBuffer,--wrap=vaDestroyBuffer \
 *       -Wl,--wrap=vaCreateConfig,--wrap=vaDestroyConfig \
 *       -Wl,--wrap=vaCreateContext,--wrap=vaDestroyContext \
 *       -Wl,--wrap=vaCreateSurfaces,--wrap=vaDestroySurfaces \
 *       -Wl,--wrap=vaCreateImage,--wrap=vaDeriveImage,--wrap=vaDestroyImage \
 *       -Wl,--wrap=vaBeginPicture,--wrap=vaEndPicture,--wrap=vaTerminate \
 *       -lva -lva-drm -o vaapi_h264_encoder_lifecycle_test &&
 *     ./vaapi_h264_encoder_lifecycle_test
 */

#include <dirent.h>
#include <va/va.h>

#include <cerrno>
#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <set>
#include <type_traits>
#include <vector>

#include "vaapi_h264_encoder_wrapper.h"

namespace {

int g_failures = 0;

void Fail(const char* format, ...) {
  std::fprintf(stderr, "FAIL: ");
  va_list args;
  va_start(args, format);
  std::vfprintf(stderr, format, args);
  va_end(args);
  std::fprintf(stderr, "\n");
  ++g_failures;
}

// VA objects created through the wrapped functions and not destroyed yet.
// The test uses one display at a time.
std::map<VABufferID, VABufferType> g_live_buffers;
std::set<VASurfaceID> g_live_surfaces;
std::set<VAImageID> g_live_images;
std::set<VAContextID> g_live_contexts;
std::set<VAConfigID> g_live_configs;
// Destroy calls for IDs that were never created or were already destroyed.
int g_unknown_destroys = 0;
int g_param_buffers_created = 0;

// When positive, counts down on each parameter (or coded) buffer creation;
// the creation that reaches zero fails without reaching the driver.
int g_fail_param_buffer_in = 0;
int g_fail_coded_buffer_in = 0;
// When set, the next vaEndPicture() submits the picture but reports failure.
bool g_fail_next_end_picture = false;
int g_injected_failures = 0;
VASurfaceID g_render_target = VA_INVALID_SURFACE;

struct HandleCounts {
  size_t param_buffers = 0;
  size_t coded_buffers = 0;
  size_t surfaces = 0;
  size_t images = 0;
  size_t contexts = 0;
  size_t configs = 0;

  size_t Total() const {
    return param_buffers + coded_buffers + surfaces + images + contexts +
           configs;
  }
};

// Handles that were still live when their display was terminated.
HandleCounts g_terminated_leaks;
// Parameter buffers already reported as outliving Encode(), so that each leak
// is reported once.
std::set<VABufferID> g_reported_param_buffers;

// Adds the live handles to `counts` and forgets them.
void MoveLiveHandlesTo(HandleCounts& counts) {
  for (const auto& buffer : g_live_buffers) {
    if (buffer.second == VAEncCodedBufferType) {
      ++counts.coded_buffers;
    } else if (g_reported_param_buffers.count(buffer.first) == 0) {
      ++counts.param_buffers;
    }
  }
  counts.surfaces += g_live_surfaces.size();
  counts.images += g_live_images.size();
  counts.contexts += g_live_contexts.size();
  counts.configs += g_live_configs.size();
  g_live_buffers.clear();
  g_live_surfaces.clear();
  g_live_images.clear();
  g_live_contexts.clear();
  g_live_configs.clear();
  g_reported_param_buffers.clear();
}

template <typename LiveSet>
VAStatus Untrack(LiveSet& live, VAGenericID id, VAStatus status) {
  if (live.count(id) == 0) {
    ++g_unknown_destroys;
  } else if (status == VA_STATUS_SUCCESS) {
    live.erase(id);
  }
  return status;
}

}  // namespace

extern "C" {

VAStatus __real_vaCreateBuffer(VADisplay dpy,
                               VAContextID context,
                               VABufferType type,
                               unsigned int size,
                               unsigned int num_elements,
                               void* data,
                               VABufferID* buf_id);
VAStatus __real_vaDestroyBuffer(VADisplay dpy, VABufferID buf_id);
VAStatus __real_vaCreateConfig(VADisplay dpy,
                               VAProfile profile,
                               VAEntrypoint entrypoint,
                               VAConfigAttrib* attrib_list,
                               int num_attribs,
                               VAConfigID* config_id);
VAStatus __real_vaDestroyConfig(VADisplay dpy, VAConfigID config_id);
VAStatus __real_vaCreateContext(VADisplay dpy,
                                VAConfigID config_id,
                                int picture_width,
                                int picture_height,
                                int flag,
                                VASurfaceID* render_targets,
                                int num_render_targets,
                                VAContextID* context);
VAStatus __real_vaDestroyContext(VADisplay dpy, VAContextID context);
VAStatus __real_vaCreateSurfaces(VADisplay dpy,
                                 unsigned int format,
                                 unsigned int width,
                                 unsigned int height,
                                 VASurfaceID* surfaces,
                                 unsigned int num_surfaces,
                                 VASurfaceAttrib* attrib_list,
                                 unsigned int num_attribs);
VAStatus __real_vaDestroySurfaces(VADisplay dpy,
                                  VASurfaceID* surfaces,
                                  int num_surfaces);
VAStatus __real_vaCreateImage(VADisplay dpy,
                              VAImageFormat* format,
                              int width,
                              int height,
                              VAImage* image);
VAStatus __real_vaDeriveImage(VADisplay dpy,
                              VASurfaceID surface,
                              VAImage* image);
VAStatus __real_vaDestroyImage(VADisplay dpy, VAImageID image);
VAStatus __real_vaBeginPicture(VADisplay dpy,
                               VAContextID context,
                               VASurfaceID render_target);
VAStatus __real_vaEndPicture(VADisplay dpy, VAContextID context);
VAStatus __real_vaTerminate(VADisplay dpy);

VAStatus __wrap_vaCreateBuffer(VADisplay dpy,
                               VAContextID context,
                               VABufferType type,
                               unsigned int size,
                               unsigned int num_elements,
                               void* data,
                               VABufferID* buf_id) {
  const bool coded = type == VAEncCodedBufferType;
  int& fail_in = coded ? g_fail_coded_buffer_in : g_fail_param_buffer_in;
  if (fail_in > 0 && --fail_in == 0) {
    ++g_injected_failures;
    return VA_STATUS_ERROR_ALLOCATION_FAILED;
  }
  VAStatus status = __real_vaCreateBuffer(dpy, context, type, size,
                                          num_elements, data, buf_id);
  if (status == VA_STATUS_SUCCESS) {
    g_live_buffers[*buf_id] = type;
    if (!coded) {
      ++g_param_buffers_created;
    }
  }
  return status;
}

VAStatus __wrap_vaDestroyBuffer(VADisplay dpy, VABufferID buf_id) {
  return Untrack(g_live_buffers, buf_id, __real_vaDestroyBuffer(dpy, buf_id));
}

VAStatus __wrap_vaCreateConfig(VADisplay dpy,
                               VAProfile profile,
                               VAEntrypoint entrypoint,
                               VAConfigAttrib* attrib_list,
                               int num_attribs,
                               VAConfigID* config_id) {
  VAStatus status = __real_vaCreateConfig(dpy, profile, entrypoint, attrib_list,
                                          num_attribs, config_id);
  if (status == VA_STATUS_SUCCESS) {
    g_live_configs.insert(*config_id);
  }
  return status;
}

VAStatus __wrap_vaDestroyConfig(VADisplay dpy, VAConfigID config_id) {
  return Untrack(g_live_configs, config_id,
                 __real_vaDestroyConfig(dpy, config_id));
}

VAStatus __wrap_vaCreateContext(VADisplay dpy,
                                VAConfigID config_id,
                                int picture_width,
                                int picture_height,
                                int flag,
                                VASurfaceID* render_targets,
                                int num_render_targets,
                                VAContextID* context) {
  VAStatus status =
      __real_vaCreateContext(dpy, config_id, picture_width, picture_height,
                             flag, render_targets, num_render_targets, context);
  if (status == VA_STATUS_SUCCESS) {
    g_live_contexts.insert(*context);
  }
  return status;
}

VAStatus __wrap_vaDestroyContext(VADisplay dpy, VAContextID context) {
  return Untrack(g_live_contexts, context,
                 __real_vaDestroyContext(dpy, context));
}

VAStatus __wrap_vaCreateSurfaces(VADisplay dpy,
                                 unsigned int format,
                                 unsigned int width,
                                 unsigned int height,
                                 VASurfaceID* surfaces,
                                 unsigned int num_surfaces,
                                 VASurfaceAttrib* attrib_list,
                                 unsigned int num_attribs) {
  VAStatus status =
      __real_vaCreateSurfaces(dpy, format, width, height, surfaces,
                              num_surfaces, attrib_list, num_attribs);
  if (status == VA_STATUS_SUCCESS) {
    g_live_surfaces.insert(surfaces, surfaces + num_surfaces);
  }
  return status;
}

VAStatus __wrap_vaDestroySurfaces(VADisplay dpy,
                                  VASurfaceID* surfaces,
                                  int num_surfaces) {
  VAStatus status = __real_vaDestroySurfaces(dpy, surfaces, num_surfaces);
  for (int i = 0; i < num_surfaces; ++i) {
    Untrack(g_live_surfaces, surfaces[i], status);
  }
  return status;
}

VAStatus __wrap_vaCreateImage(VADisplay dpy,
                              VAImageFormat* format,
                              int width,
                              int height,
                              VAImage* image) {
  VAStatus status = __real_vaCreateImage(dpy, format, width, height, image);
  if (status == VA_STATUS_SUCCESS) {
    g_live_images.insert(image->image_id);
  }
  return status;
}

VAStatus __wrap_vaDeriveImage(VADisplay dpy,
                              VASurfaceID surface,
                              VAImage* image) {
  VAStatus status = __real_vaDeriveImage(dpy, surface, image);
  if (status == VA_STATUS_SUCCESS) {
    g_live_images.insert(image->image_id);
  }
  return status;
}

VAStatus __wrap_vaDestroyImage(VADisplay dpy, VAImageID image) {
  return Untrack(g_live_images, image, __real_vaDestroyImage(dpy, image));
}

VAStatus __wrap_vaBeginPicture(VADisplay dpy,
                               VAContextID context,
                               VASurfaceID render_target) {
  g_render_target = render_target;
  return __real_vaBeginPicture(dpy, context, render_target);
}

VAStatus __wrap_vaEndPicture(VADisplay dpy, VAContextID context) {
  VAStatus status = __real_vaEndPicture(dpy, context);
  if (!g_fail_next_end_picture) {
    return status;
  }
  g_fail_next_end_picture = false;
  if (status != VA_STATUS_SUCCESS) {
    Fail("vaEndPicture() failed before a failure could be injected: %d",
         status);
    return status;
  }
  // The picture was submitted. Wait for it, so that teardown does not race
  // the hardware, then report failure.
  status = vaSyncSurface(dpy, g_render_target);
  if (status != VA_STATUS_SUCCESS) {
    Fail("vaSyncSurface() failed after vaEndPicture(): %d", status);
    return status;
  }
  ++g_injected_failures;
  return VA_STATUS_ERROR_OPERATION_FAILED;
}

VAStatus __wrap_vaTerminate(VADisplay dpy) {
  // Record handles that were not destroyed before termination. The next
  // display may reuse their IDs.
  MoveLiveHandlesTo(g_terminated_leaks);
  return __real_vaTerminate(dpy);
}

}  // extern "C"

// The hand-written prototypes above must match libva's declarations.
#define CHECK_WRAPPED_SIGNATURE(function)                                    \
  static_assert(                                                             \
      std::is_same_v<decltype(&__wrap_##function), decltype(&function)> &&   \
          std::is_same_v<decltype(&__real_##function), decltype(&function)>, \
      "the " #function " wrappers do not match libva's signature")
CHECK_WRAPPED_SIGNATURE(vaCreateBuffer);
CHECK_WRAPPED_SIGNATURE(vaDestroyBuffer);
CHECK_WRAPPED_SIGNATURE(vaCreateConfig);
CHECK_WRAPPED_SIGNATURE(vaDestroyConfig);
CHECK_WRAPPED_SIGNATURE(vaCreateContext);
CHECK_WRAPPED_SIGNATURE(vaDestroyContext);
CHECK_WRAPPED_SIGNATURE(vaCreateSurfaces);
CHECK_WRAPPED_SIGNATURE(vaDestroySurfaces);
CHECK_WRAPPED_SIGNATURE(vaCreateImage);
CHECK_WRAPPED_SIGNATURE(vaDeriveImage);
CHECK_WRAPPED_SIGNATURE(vaDestroyImage);
CHECK_WRAPPED_SIGNATURE(vaBeginPicture);
CHECK_WRAPPED_SIGNATURE(vaEndPicture);
CHECK_WRAPPED_SIGNATURE(vaTerminate);

namespace {

using livekit_ffi::VaapiH264EncoderWrapper;

constexpr int kWidth = 640;
constexpr int kHeight = 480;
constexpr int kFrameRate = 30;
// Shorter than the encoder factory's 5 seconds, so IDR frames recur in a
// short run.
constexpr int kKeyFrameInterval = 30;

int g_fd_baseline = 0;

void Expect(bool condition, const char* what) {
  if (!condition) {
    Fail("%s", what);
  }
}

// Returns the number of entries in /proc/self/fd, or -1 if it cannot be read.
int CountOpenFds() {
  DIR* dir = opendir("/proc/self/fd");
  if (!dir) {
    return -1;
  }
  int count = 0;
  errno = 0;
  while (readdir(dir)) {
    ++count;
  }
  const bool read_failed = errno != 0;
  closedir(dir);
  return read_failed ? -1 : count;
}

size_t LiveBuffers(bool coded) {
  size_t count = 0;
  for (const auto& buffer : g_live_buffers) {
    if ((buffer.second == VAEncCodedBufferType) == coded) {
      ++count;
    }
  }
  return count;
}

size_t LiveParamBuffers() {
  return LiveBuffers(/*coded=*/false);
}

size_t LiveCodedBuffers() {
  return LiveBuffers(/*coded=*/true);
}

// Reports live parameter buffers once; they stay tracked until teardown.
void MarkParamBuffersReported() {
  for (const auto& buffer : g_live_buffers) {
    if (buffer.second != VAEncCodedBufferType) {
      g_reported_param_buffers.insert(buffer.first);
    }
  }
}

// Checks that teardown destroyed every handle before terminating the display
// and closed the display's file descriptor.
void ExpectAllReleased(const char* when) {
  // Handles still live here were not released by a display termination.
  HandleCounts leaks = g_terminated_leaks;
  MoveLiveHandlesTo(leaks);
  g_terminated_leaks = {};
  const int unknown_destroys = g_unknown_destroys;
  g_unknown_destroys = 0;
  const int open_fds = CountOpenFds();
  if (open_fds < 0) {
    Fail("%s: cannot read /proc/self/fd", when);
  }
  const int leaked_fds = open_fds < 0 ? 0 : open_fds - g_fd_baseline;
  if (leaks.Total() == 0 && unknown_destroys == 0 && leaked_fds == 0) {
    return;
  }
  Fail(
      "%s: not destroyed: %zu parameter buffers, %zu coded buffers, %zu "
      "surfaces, %zu images, %zu contexts, %zu configs; %d unknown destroys; "
      "%d file descriptors over baseline",
      when, leaks.param_buffers, leaks.coded_buffers, leaks.surfaces,
      leaks.images, leaks.contexts, leaks.configs, unknown_destroys,
      leaked_fds);
  // Report each leaked file descriptor once.
  g_fd_baseline += leaked_fds;
}

bool Initialize(VaapiH264EncoderWrapper& encoder) {
  // Same arguments as VAAPIH264EncoderWrapper::InitEncode() passes, apart
  // from the key frame interval.
  return encoder.Initialize(kWidth, kHeight, /*bitrate=*/1000 * 1000,
                            kKeyFrameInterval, kKeyFrameInterval,
                            /*ip_period=*/1, kFrameRate,
                            VAProfileH264ConstrainedBaseline, VA_RC_CBR);
}

// Returns whether Encode() succeeded with output. `encode_result`, if set,
// receives Encode()'s own return value.
bool EncodeFrame(VaapiH264EncoderWrapper& encoder,
                 bool force_idr,
                 bool* encode_result = nullptr) {
  static int frame_index = 0;
  static std::vector<uint8_t> y(kWidth * kHeight);
  static std::vector<uint8_t> u(kWidth * kHeight / 4, 128);
  static std::vector<uint8_t> v(kWidth * kHeight / 4, 128);
  // A moving gradient, so that consecutive frames differ.
  for (int row = 0; row < kHeight; ++row) {
    for (int col = 0; col < kWidth; ++col) {
      y[row * kWidth + col] = static_cast<uint8_t>(row + col + frame_index);
    }
  }
  ++frame_index;
  std::vector<uint8_t> output;
  const bool result = encoder.Encode(VA_FOURCC_I420, y.data(), u.data(),
                                     v.data(), force_idr, output);
  if (encode_result) {
    *encode_result = result;
  }
  return result && !output.empty();
}

// Encodes `frames` frames and checks that no parameter buffer outlives an
// Encode() call.
void EncodeAndCheck(VaapiH264EncoderWrapper& encoder,
                    int frames,
                    const char* when) {
  const int created_before = g_param_buffers_created;
  int encoded = 0;
  int first_leaking_frame = -1;
  for (int frame = 0; frame < frames; ++frame) {
    encoded += EncodeFrame(encoder, /*force_idr=*/false);
    if (first_leaking_frame < 0 && LiveParamBuffers() != 0) {
      first_leaking_frame = frame;
    }
  }
  if (encoded != frames) {
    Fail("%s: %d of %d frames encoded", when, encoded, frames);
  }
  if (first_leaking_frame >= 0) {
    Fail(
        "%s: parameter buffers outlive Encode(), from frame %d on (%zu live "
        "after %d frames)",
        when, first_leaking_frame, LiveParamBuffers(), frames);
    MarkParamBuffersReported();
  }
  // At least picture and slice parameters on every frame.
  if (g_param_buffers_created - created_before < 2 * frames) {
    Fail("%s: too few parameter buffers created", when);
  }
  if (LiveCodedBuffers() != SURFACE_NUM) {
    Fail("%s: %zu coded buffers live, expected %d", when, LiveCodedBuffers(),
         SURFACE_NUM);
  }
}

// Repeated initialize -> encode -> teardown, alternating Destroy() and the
// destructor. Initialize() on a live encoder must release it first.
void TestRepeatedEncodeAndTeardown() {
  constexpr int kFrames = 2 * kKeyFrameInterval + 10;  // Three IDR frames.
  for (int cycle = 1; cycle <= 3; ++cycle) {
    {
      VaapiH264EncoderWrapper encoder;
      if (!Initialize(encoder)) {
        Fail("cycle %d: Initialize() failed", cycle);
        return;
      }
      EncodeAndCheck(encoder, kFrames, "first initialization");
      if (!Initialize(encoder)) {
        Fail("cycle %d: Initialize() on a live encoder failed", cycle);
        return;
      }
      EncodeAndCheck(encoder, 5, "reinitialization of a live encoder");
      if (cycle % 2 == 1) {
        encoder.Destroy();
      }
    }
    ExpectAllReleased("after teardown");
  }
}

enum class Fault { kParamBufferAllocation, kEndPicture };

// After one valid frame, makes a VA call fail during the next frame. Whatever
// Encode() returns, no parameter buffer may outlive it. The encoder is torn
// down rather than reused, because recovering from an incomplete picture is
// up to the driver.
void TestFaultedFrame(const char* name, Fault fault, bool force_idr) {
  {
    VaapiH264EncoderWrapper encoder;
    if (!Initialize(encoder)) {
      Fail("%s: Initialize() failed", name);
      return;
    }
    if (!EncodeFrame(encoder, /*force_idr=*/false)) {
      Fail("%s: the frame before the fault did not encode", name);
    }
    const int created_before = g_param_buffers_created;
    const int injected_before = g_injected_failures;
    if (fault == Fault::kParamBufferAllocation) {
      // The first parameter buffer of the frame is created; the second fails.
      g_fail_param_buffer_in = 2;
    } else {
      g_fail_next_end_picture = true;
    }
    bool encode_result = false;
    EncodeFrame(encoder, force_idr, &encode_result);
    g_fail_param_buffer_in = 0;
    g_fail_next_end_picture = false;

    if (g_injected_failures != injected_before + 1) {
      Fail("%s: the failure was not injected", name);
    }
    if (fault == Fault::kEndPicture && encode_result) {
      Fail("%s: Encode() returned true although vaEndPicture() failed", name);
    }
    if (g_param_buffers_created == created_before) {
      Fail("%s: no parameter buffer was created before the failure", name);
    }
    if (LiveParamBuffers() != 0) {
      Fail("%s: %zu parameter buffers outlive Encode()", name,
           LiveParamBuffers());
      MarkParamBuffersReported();
    }
    if (LiveCodedBuffers() != SURFACE_NUM) {
      Fail("%s: %zu coded buffers live, expected %d", name, LiveCodedBuffers(),
           SURFACE_NUM);
    }
    encoder.Destroy();
  }
  ExpectAllReleased(name);
}

// A coded buffer allocation fails inside Initialize(). Everything created
// before it must be released, and the same wrapper must then initialize and
// encode normally.
void TestCodedBufferFailureDuringInitialize() {
  {
    VaapiH264EncoderWrapper encoder;
    const int injected_before = g_injected_failures;
    g_fail_coded_buffer_in = SURFACE_NUM / 2;
    Expect(!Initialize(encoder),
           "Initialize() fails when a coded buffer cannot be created");
    g_fail_coded_buffer_in = 0;
    Expect(g_injected_failures == injected_before + 1,
           "a coded buffer allocation failure was injected");
    ExpectAllReleased("after the failed Initialize()");

    if (!Initialize(encoder)) {
      Fail("Initialize() after a failed Initialize() failed");
      return;
    }
    EncodeAndCheck(encoder, 10, "after a failed Initialize()");
  }
  ExpectAllReleased("after destruction following a failed Initialize()");
}

}  // namespace

int main() {
  {
    livekit_ffi::VaapiDisplayDrm display;
    if (!display.Open()) {
      std::printf("SKIP: no VAAPI device with H.264 encode support\n");
      return EXIT_SUCCESS;
    }
  }
  // Encode once before taking the file descriptor baseline, so that
  // driver-global state set up on first use does not count as a leak.
  {
    VaapiH264EncoderWrapper encoder;
    if (!Initialize(encoder) || !EncodeFrame(encoder, /*force_idr=*/false)) {
      Fail("warm-up: Initialize() or Encode() failed");
    }
  }
  g_fd_baseline = CountOpenFds();
  if (g_fd_baseline < 0) {
    Fail("cannot read /proc/self/fd");
  }
  ExpectAllReleased("after warm-up");

  TestRepeatedEncodeAndTeardown();
  TestFaultedFrame("P frame, parameter buffer allocation fails",
                   Fault::kParamBufferAllocation, /*force_idr=*/false);
  TestFaultedFrame("IDR frame, parameter buffer allocation fails",
                   Fault::kParamBufferAllocation, /*force_idr=*/true);
  TestFaultedFrame("P frame, vaEndPicture() fails", Fault::kEndPicture,
                   /*force_idr=*/false);
  TestFaultedFrame("IDR frame, vaEndPicture() fails", Fault::kEndPicture,
                   /*force_idr=*/true);
  TestCodedBufferFailureDuringInitialize();

  if (g_failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", g_failures);
    return EXIT_FAILURE;
  }
  std::printf(
      "All vaapi_h264_encoder_lifecycle tests passed (%d parameter buffers "
      "created)\n",
      g_param_buffers_created);
  return EXIT_SUCCESS;
}
