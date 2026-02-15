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

#include "mpp_encoder_factory.h"

#include <memory>

#include "h264_encoder_impl.h"
#include "h265_encoder_impl.h"
#include "mpp_context.h"
#include "rtc_base/logging.h"

namespace webrtc {

MppVideoEncoderFactory::MppVideoEncoderFactory() {
  // Advertise H.264 Constrained Baseline profile
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  // Check if H.265/HEVC encoding is supported on this SoC
  if (mpp_check_support_format(MPP_CTX_ENC, MPP_VIDEO_CodingHEVC) == MPP_OK) {
    supported_formats_.push_back(SdpVideoFormat("H265"));
    supported_formats_.push_back(SdpVideoFormat("HEVC"));
  }
}

MppVideoEncoderFactory::~MppVideoEncoderFactory() {}

bool MppVideoEncoderFactory::IsSupported() {
  return livekit_ffi::MppContext::IsAvailable();
}

std::unique_ptr<VideoEncoder> MppVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using Rockchip MPP HW encoder for H264";
        return std::make_unique<MppH264EncoderImpl>(env, format);
      }

      if (format.name == "H265" || format.name == "HEVC") {
        RTC_LOG(LS_INFO) << "Using Rockchip MPP HW encoder for H265/HEVC";
        return std::make_unique<MppH265EncoderImpl>(env, format);
      }
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> MppVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> MppVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
