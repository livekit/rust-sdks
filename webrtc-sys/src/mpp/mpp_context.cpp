/*
 * Copyright 2025 LiveKit, Inc.
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

#include "mpp_context.h"

#include <dlfcn.h>
#include <unistd.h>

#include <rockchip/rk_mpi.h>

#include "rtc_base/logging.h"

namespace livekit_ffi {

bool MppContext::IsAvailable() {
  // Probe for the MPP library via dlopen. The lazy-load trampoline stubs
  // will handle the actual symbol resolution, but we need to verify the
  // library exists on the system first.
  void* handle = dlopen("librockchip_mpp.so", RTLD_LAZY | RTLD_GLOBAL);
  if (!handle) {
    RTC_LOG(LS_INFO) << "librockchip_mpp.so not found: " << dlerror();
    return false;
  }

  const bool has_buffer_sync =
      dlsym(handle, "mpp_buffer_sync_begin_f") != nullptr &&
      dlsym(handle, "mpp_buffer_sync_end_f") != nullptr;
  if (!has_buffer_sync) {
    RTC_LOG(LS_WARNING)
        << "librockchip_mpp.so does not provide cache synchronization APIs.";
    dlclose(handle);
    return false;
  }

  // Close immediately -- the implib lazy-load stubs will re-dlopen when
  // individual MPP functions are first called.
  dlclose(handle);

  // Additionally check that an MPP kernel service node is usable by this
  // process, rather than merely present on the filesystem.
  const bool has_mpp_service =
      (access("/dev/mpp_service", R_OK | W_OK) == 0);
  const bool has_vpu_service =
      (access("/dev/vpu_service", R_OK | W_OK) == 0);
  const bool has_vpu_combo =
      (access("/dev/vpu-service", R_OK | W_OK) == 0);

  if (!has_mpp_service && !has_vpu_service && !has_vpu_combo) {
    RTC_LOG(LS_INFO)
        << "No accessible Rockchip VPU/MPP service device node found.";
    return false;
  }

  // Try to verify the encoder is actually functional by checking codec support.
  MPP_RET ret = mpp_check_support_format(MPP_CTX_ENC, MPP_VIDEO_CodingAVC);
  if (ret != MPP_OK) {
    RTC_LOG(LS_WARNING) << "Rockchip MPP does not support H.264 encoding on this SoC.";
    return false;
  }

  return true;
}
}  // namespace livekit_ffi
