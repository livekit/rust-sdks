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

#include "mf_common.h"

// Instantiate the CODECAPI_* GUIDs (codecapi.h only declares them unless
// INITGUID is in effect). The definitions are DECLSPEC_SELECTANY, so a single
// translation unit doing this is sufficient and safe; every other file
// includes codecapi.h without initguid.h and links against these.
#include <initguid.h>

#include <codecapi.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <type_traits>

#include "rtc_base/logging.h"

namespace livekit_ffi {

namespace {

struct ThreadComInit {
  bool ok = false;
  bool should_uninit = false;

  ThreadComInit() {
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (SUCCEEDED(hr)) {
      // S_FALSE means COM was already initialized on this thread; either way
      // this thread now owns a reference that must be released on exit.
      ok = true;
      should_uninit = true;
    } else if (hr == RPC_E_CHANGED_MODE) {
      // Thread is already in an STA; the MF objects used here are
      // free-threaded, so proceed without taking a reference.
      ok = true;
    }
  }

  ~ThreadComInit() {
    if (should_uninit) {
      CoUninitialize();
    }
  }
};

}  // namespace

const MfFunctions* GetMfFunctions() {
  static const MfFunctions* loaded = []() -> const MfFunctions* {
    HMODULE mod =
        LoadLibraryExW(L"mfplat.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!mod) {
      RTC_LOG(LS_INFO)
          << "mfplat.dll unavailable; MediaFoundation hardware video codec "
             "disabled.";
      return nullptr;
    }

    static MfFunctions funcs;
    auto resolve = [&](auto& fn, const char* name) -> bool {
      fn = reinterpret_cast<std::remove_reference_t<decltype(fn)>>(
          GetProcAddress(mod, name));
      if (!fn) {
        RTC_LOG(LS_WARNING) << "mfplat.dll missing export " << name
                            << "; MediaFoundation hardware video codec "
                               "disabled.";
        return false;
      }
      return true;
    };

    if (!resolve(funcs.startup, "MFStartup") ||
        !resolve(funcs.mft_enum_ex, "MFTEnumEx") ||
        !resolve(funcs.create_media_type, "MFCreateMediaType") ||
        !resolve(funcs.create_sample, "MFCreateSample") ||
        !resolve(funcs.create_memory_buffer, "MFCreateMemoryBuffer") ||
        !resolve(funcs.create_aligned_memory_buffer,
                 "MFCreateAlignedMemoryBuffer") ||
        !resolve(funcs.create_2d_media_buffer, "MFCreate2DMediaBuffer") ||
        !resolve(funcs.create_dxgi_device_manager,
                 "MFCreateDXGIDeviceManager")) {
      FreeLibrary(mod);
      return nullptr;
    }

    // Keep mfplat.dll pinned for the process lifetime while MF objects live.
    return &funcs;
  }();
  return loaded;
}

bool EnsureComInitialized() {
  thread_local ThreadComInit init;
  return init.ok;
}

bool EnsureMFStarted() {
  static bool ok = [] {
    const MfFunctions* mf = GetMfFunctions();
    if (!mf) {
      return false;
    }
    HRESULT hr = mf->startup(MF_VERSION, MFSTARTUP_LITE);
    if (FAILED(hr)) {
      RTC_LOG(LS_ERROR) << "MFStartup failed: " << HResultToString(hr);
      return false;
    }
    return true;
  }();
  return ok;
}

std::string HResultToString(HRESULT hr) {
  char buf[16];
  std::snprintf(buf, sizeof(buf), "0x%08lX", static_cast<unsigned long>(hr));
  return buf;
}

bool IsMfDisabledByEnv() {
  const char* val = std::getenv("LK_DISABLE_MF");
  if (!val || val[0] == '\0' || std::strcmp(val, "0") == 0) {
    return false;
  }
  static std::once_flag log_once;
  std::call_once(log_once, [] {
    RTC_LOG(LS_INFO)
        << "MediaFoundation hardware video codec disabled because "
           "LK_DISABLE_MF is set.";
  });
  return true;
}

}  // namespace livekit_ffi
