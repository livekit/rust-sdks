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

#include "mf_decoder_factory.h"

#include <d3d11.h>
#include <wmcodecdsp.h>

#include <memory>

#include <modules/video_coding/codecs/h264/include/h264.h>

#include "h264_decoder_impl.h"
#include "mf_common.h"
#include "rtc_base/logging.h"

namespace webrtc {

using livekit_ffi::ComPtr;

namespace {

// The inbox decoder MFT ships with every Windows 10/11, but it only decodes
// in hardware when a video-capable D3D11 device exists. Without one the MFT
// would decode in software on the CPU — no better than the built-in decoder —
// so both are required before this factory registers itself.
bool ProbeDecodeSupport() {
  if (!livekit_ffi::EnsureComInitialized() || !livekit_ffi::EnsureMFStarted()) {
    return false;
  }

  ComPtr<IMFTransform> transform;
  HRESULT hr =
      CoCreateInstance(CLSID_CMSH264DecoderMFT, nullptr, CLSCTX_INPROC_SERVER,
                       IID_PPV_ARGS(&transform));
  if (FAILED(hr)) {
    RTC_LOG(LS_WARNING) << "Inbox H264 decoder MFT unavailable: "
                        << livekit_ffi::HResultToString(hr);
    return false;
  }

  ComPtr<ID3D11Device> device;
  hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                         D3D11_CREATE_DEVICE_VIDEO_SUPPORT, nullptr, 0,
                         D3D11_SDK_VERSION, &device, nullptr, nullptr);
  if (FAILED(hr)) {
    RTC_LOG(LS_WARNING)
        << "No video-capable D3D11 device; MF hardware decode unavailable: "
        << livekit_ffi::HResultToString(hr);
    return false;
  }
  return true;
}

}  // namespace

MFVideoDecoderFactory::MFVideoDecoderFactory() {
  // DXVA H264 decode is >= level 4.1 on any hardware that gets this far;
  // advertise 5.1 like the NVDEC factory. Both packetization modes are
  // supported (the depacketized Annex-B stream looks the same to the MFT).
  for (const char* packetization_mode : {"1", "0"}) {
    supported_formats_.push_back(
        CreateH264Format(webrtc::H264Profile::kProfileConstrainedBaseline,
                         webrtc::H264Level::kLevel5_1, packetization_mode));
    supported_formats_.push_back(
        CreateH264Format(webrtc::H264Profile::kProfileBaseline,
                         webrtc::H264Level::kLevel5_1, packetization_mode));
    supported_formats_.push_back(
        CreateH264Format(webrtc::H264Profile::kProfileMain,
                         webrtc::H264Level::kLevel5_1, packetization_mode));
    supported_formats_.push_back(
        CreateH264Format(webrtc::H264Profile::kProfileHigh,
                         webrtc::H264Level::kLevel5_1, packetization_mode));
  }
}

MFVideoDecoderFactory::~MFVideoDecoderFactory() {}

bool MFVideoDecoderFactory::IsSupported() {
  static const bool supported = [] {
    if (!ProbeDecodeSupport()) {
      return false;
    }
    RTC_LOG(LS_INFO) << "MediaFoundation hardware H264 decoder is available.";
    return true;
  }();
  return supported;
}

std::unique_ptr<VideoDecoder> MFVideoDecoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using MediaFoundation HW decoder (DXVA) for H264";
        return std::make_unique<MFH264DecoderImpl>();
      }
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> MFVideoDecoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

}  // namespace webrtc
