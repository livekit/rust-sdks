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

#include "mf_encoder_factory.h"

#include <memory>

#include "api/video_codecs/h264_profile_level_id.h"
#include "h264_encoder_impl.h"
#include "mf_common.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "rtc_base/logging.h"

namespace webrtc {

using livekit_ffi::ComPtr;
using livekit_ffi::HResultToString;

namespace {

// A hardware encoder MFT activated for probing. ShutdownObject() runs on
// destruction, as IMFActivate::ActivateObject requires.
struct ProbeTransform {
  ComPtr<IMFActivate> activate;
  ComPtr<IMFTransform> transform;

  explicit operator bool() const { return transform != nullptr; }

  ~ProbeTransform() {
    transform.Reset();
    if (activate) {
      activate->ShutdownObject();
    }
  }
};

// Activates the first (best, thanks to MFT_ENUM_FLAG_SORTANDFILTER) hardware
// H264 encoder MFT, or an empty result when none is present or activation
// fails. The presence of a registration alone is not enough — old or broken
// drivers register MFTs that fail to activate, mirroring the NVENC
// open-a-session probe.
ProbeTransform ActivateFirstHwEncoder() {
  ProbeTransform result;
  if (!livekit_ffi::EnsureComInitialized() || !livekit_ffi::EnsureMFStarted()) {
    return result;
  }

  MFT_REGISTER_TYPE_INFO input_info = {MFMediaType_Video, MFVideoFormat_NV12};
  MFT_REGISTER_TYPE_INFO output_info = {MFMediaType_Video, MFVideoFormat_H264};

  IMFActivate** activates = nullptr;
  UINT32 count = 0;
  HRESULT hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,
                         MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                         &input_info, &output_info, &activates, &count);
  if (FAILED(hr) || count == 0) {
    if (activates) {
      CoTaskMemFree(activates);
    }
    return result;
  }

  result.activate = activates[0];
  for (UINT32 i = 0; i < count; i++) {
    activates[i]->Release();
  }
  CoTaskMemFree(activates);

  hr = result.activate->ActivateObject(IID_PPV_ARGS(&result.transform));
  if (FAILED(hr)) {
    RTC_LOG(LS_WARNING) << "Hardware H264 encoder MFT failed to activate: "
                        << HResultToString(hr);
    result.transform.Reset();
  }
  return result;
}

// Determines the highest H264 level (of the ones we care about) the MFT
// accepts, by offering output media types with MFT_SET_TYPE_TEST_ONLY.
// Level 4.0 is the minimum for 1080p30 — the point of this backend — with
// 4.2 preferred; 3.1 is the compatibility floor.
H264Level ProbeMaxLevel(IMFTransform* transform) {
  ComPtr<IMFAttributes> attributes;
  if (SUCCEEDED(transform->GetAttributes(&attributes)) && attributes) {
    UINT32 is_async = 0;
    attributes->GetUINT32(MF_TRANSFORM_ASYNC, &is_async);
    if (is_async) {
      // Media types cannot be set (even TEST_ONLY) while the async MFT is
      // locked.
      attributes->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
    }
  }

  DWORD input_ids[1] = {0};
  DWORD output_ids[1] = {0};
  if (transform->GetStreamIDs(1, input_ids, 1, output_ids) == E_NOTIMPL) {
    output_ids[0] = 0;
  }

  struct Candidate {
    H264Level level;
    UINT32 mf_level;
    UINT32 width;
    UINT32 height;
  };
  const Candidate candidates[] = {
      {H264Level::kLevel4_2, eAVEncH264VLevel4_2, 1920, 1080},
      {H264Level::kLevel4, eAVEncH264VLevel4, 1920, 1080},
  };

  for (const Candidate& candidate : candidates) {
    ComPtr<IMFMediaType> type;
    if (FAILED(MFCreateMediaType(&type))) {
      break;
    }
    type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_H264);
    type->SetUINT32(MF_MT_AVG_BITRATE, 5'000'000);
    MFSetAttributeSize(type.Get(), MF_MT_FRAME_SIZE, candidate.width,
                       candidate.height);
    MFSetAttributeRatio(type.Get(), MF_MT_FRAME_RATE, 30, 1);
    type->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    type->SetUINT32(MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base);
    type->SetUINT32(MF_MT_MPEG2_LEVEL, candidate.mf_level);

    HRESULT hr = transform->SetOutputType(output_ids[0], type.Get(),
                                          MFT_SET_TYPE_TEST_ONLY);
    if (SUCCEEDED(hr)) {
      return candidate.level;
    }
  }

  RTC_LOG(LS_WARNING) << "Hardware H264 encoder MFT rejected level 4.0+; "
                         "advertising level 3.1 (720p cap).";
  return H264Level::kLevel3_1;
}

}  // namespace

MFVideoEncoderFactory::MFVideoEncoderFactory() {
  ComPtr<IMFTransform> transform = ActivateFirstHwEncoder();
  if (!transform) {
    return;
  }

  // Unlike the NVENC factory's hardcoded 42e01f (level 3.1, a 720p30 cap),
  // advertise the level the hardware actually accepts so 1080p can be
  // negotiated. Baseline-family only for now, matching the software H264
  // default; High is a possible later addition.
  const H264Level level = ProbeMaxLevel(transform.Get());
  supported_formats_.push_back(CreateH264Format(
      H264Profile::kProfileConstrainedBaseline, level, "1"));
}

MFVideoEncoderFactory::~MFVideoEncoderFactory() {}

bool MFVideoEncoderFactory::IsSupported() {
  // Enumeration + activation can take tens of milliseconds; the answer cannot
  // change within the process lifetime, so probe once.
  static const bool supported = [] {
    ComPtr<IMFTransform> transform = ActivateFirstHwEncoder();
    if (!transform) {
      RTC_LOG(LS_WARNING)
          << "No usable hardware H264 encoder MFT; MF encoding disabled.";
      return false;
    }
    RTC_LOG(LS_INFO) << "MediaFoundation hardware H264 encoder is available.";
    return true;
  }();
  return supported;
}

std::unique_ptr<VideoEncoder> MFVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using MediaFoundation HW encoder for H264";
        return std::make_unique<MFH264EncoderImpl>(env, format);
      }
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> MFVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> MFVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
