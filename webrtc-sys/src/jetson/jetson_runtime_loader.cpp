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

#include "jetson_runtime_loader.h"

#include <dlfcn.h>

#include <string>

namespace {

void* TryLoad(const std::string& name) {
  // Same flags the implib-generated loaders would use themselves.
  return dlopen(name.c_str(), RTLD_LAZY | RTLD_GLOBAL);
}

void* LoadWithFallbacks(const char* lib_name) {
  // L4T puts its userspace libraries in a vendor directory that is normally
  // in ld.so.conf, but not always in minimal/containerized rootfs setups:
  // R34+ uses .../nvidia, older releases used .../tegra.
  static const char* const kNvidiaLibDirs[] = {
      "/usr/lib/aarch64-linux-gnu/nvidia/",
      "/usr/lib/aarch64-linux-gnu/tegra/",
  };

  const std::string name(lib_name);
  if (void* handle = TryLoad(name)) {
    return handle;
  }
  for (const char* dir : kNvidiaLibDirs) {
    if (void* handle = TryLoad(dir + name)) {
      return handle;
    }
  }

  // The stubs bake in the SONAME of the library they were generated from
  // (e.g. libnvbufsurface.so.1.0.0); if an L4T release changes the version
  // suffix, fall back to the unversioned dev name.
  const size_t so_pos = name.rfind(".so.");
  if (so_pos != std::string::npos) {
    const std::string unversioned = name.substr(0, so_pos + 3);
    if (void* handle = TryLoad(unversioned)) {
      return handle;
    }
    for (const char* dir : kNvidiaLibDirs) {
      if (void* handle = TryLoad(dir + unversioned)) {
        return handle;
      }
    }
  }

  return nullptr;
}

}  // namespace

extern "C" void* lk_jetson_dlopen(const char* lib_name) {
  return LoadWithFallbacks(lib_name);
}

extern "C" int lk_jetson_runtime_libs_available(void) {
  static const bool available =
      LoadWithFallbacks("libnvbufsurface.so.1.0.0") != nullptr &&
      LoadWithFallbacks("libv4l2.so.0") != nullptr;
  return available ? 1 : 0;
}
