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

#ifndef WEBRTC_MF_COMMON_H_
#define WEBRTC_MF_COMMON_H_

#include <windows.h>

#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mftransform.h>
#include <strmif.h>  // ICodecAPI
#include <wrl/client.h>

#include <cstdint>
#include <string>

namespace livekit_ffi {

using Microsoft::WRL::ComPtr;

struct MfFunctions {
  decltype(&::MFStartup) startup = nullptr;
  decltype(&::MFTEnumEx) mft_enum_ex = nullptr;
  decltype(&::MFCreateMediaType) create_media_type = nullptr;
  decltype(&::MFCreateSample) create_sample = nullptr;
  decltype(&::MFCreateMemoryBuffer) create_memory_buffer = nullptr;
  decltype(&::MFCreateAlignedMemoryBuffer) create_aligned_memory_buffer =
      nullptr;
  decltype(&::MFCreate2DMediaBuffer) create_2d_media_buffer = nullptr;
  decltype(&::MFCreateDXGIDeviceManager) create_dxgi_device_manager = nullptr;
};

// Dynamically loads mfplat.dll from System32 and resolves the required MF
// entry points. Returns nullptr on editions without Media Foundation (such as
// Windows N without the Media Feature Pack) so binaries still load cleanly.
const MfFunctions* GetMfFunctions();

// Ensures COM is initialized (MTA) on the calling thread. webrtc invokes the
// encoder/decoder on its own task-queue threads which are not guaranteed to
// have called CoInitializeEx. The initialization is dropped automatically when
// the thread exits. Safe to call repeatedly and from threads that already
// initialized COM in either apartment mode.
bool EnsureComInitialized();

// Process-wide Media Foundation startup. MFStartup is called on first use and
// intentionally never balanced with MFShutdown: encoders/decoders are created
// and destroyed on different webrtc threads throughout the process lifetime,
// and tearing MF down while another thread is mid-create is racy. The OS
// reclaims MF state at process exit.
bool EnsureMFStarted();

// Formats an HRESULT as "0x8007000E" for log output.
std::string HResultToString(HRESULT hr);

// Returns true when LK_DISABLE_MF is set to any non-empty value other than "0".
bool IsMfDisabledByEnv();

}  // namespace livekit_ffi

#endif  // WEBRTC_MF_COMMON_H_
