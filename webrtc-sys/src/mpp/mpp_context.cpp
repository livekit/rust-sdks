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
#include <sys/stat.h>

#include <iostream>

#include "rtc_base/logging.h"

namespace livekit_ffi {

static bool s_library_loaded = false;

bool MppContext::LoadLibrary() {
  if (s_library_loaded)
    return true;

  // Probe for the MPP library via dlopen. The lazy-load trampoline stubs
  // will handle the actual symbol resolution, but we need to verify the
  // library exists on the system first.
  void* handle = dlopen("librockchip_mpp.so", RTLD_LAZY | RTLD_GLOBAL);
  if (!handle) {
    RTC_LOG(LS_INFO) << "librockchip_mpp.so not found: " << dlerror();
    return false;
  }

  // Close immediately -- the implib lazy-load stubs will re-dlopen when
  // individual MPP functions are first called.
  dlclose(handle);

  s_library_loaded = true;
  return true;
}

bool MppContext::IsAvailable() {
  if (!LoadLibrary()) {
    return false;
  }

  // Additionally check for the MPP kernel service device nodes.
  struct stat st;
  bool has_mpp_service = (stat("/dev/mpp_service", &st) == 0);
  bool has_vpu_service = (stat("/dev/vpu_service", &st) == 0);
  bool has_vpu_combo = (stat("/dev/vpu-service", &st) == 0);

  if (!has_mpp_service && !has_vpu_service && !has_vpu_combo) {
    RTC_LOG(LS_INFO) << "No Rockchip VPU/MPP service device node found.";
    return false;
  }

  // Try to verify the encoder is actually functional by checking codec support.
  MPP_RET ret = mpp_check_support_format(MPP_CTX_ENC, MPP_VIDEO_CodingAVC);
  if (ret != MPP_OK) {
    RTC_LOG(LS_WARNING) << "Rockchip MPP does not support H.264 encoding on this SoC.";
    return false;
  }

  std::cout << "Rockchip MPP encoder is supported." << std::endl;
  return true;
}

MppContext* MppContext::GetInstance() {
  static MppContext instance;
  return &instance;
}

}  // namespace livekit_ffi
