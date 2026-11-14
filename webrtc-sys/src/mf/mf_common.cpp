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

bool EnsureComInitialized() {
  thread_local ThreadComInit init;
  return init.ok;
}

bool EnsureMFStarted() {
  static bool ok = [] {
    HRESULT hr = MFStartup(MF_VERSION, MFSTARTUP_LITE);
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

}  // namespace livekit_ffi
