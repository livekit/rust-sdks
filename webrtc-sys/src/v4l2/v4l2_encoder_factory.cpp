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

#include "v4l2_encoder_factory.h"

#include <iostream>
#include <memory>

#include "h264_encoder_impl.h"
#include "v4l2_h264_encoder_wrapper.h"
#include "rtc_base/logging.h"

namespace webrtc {

V4L2VideoEncoderFactory::V4L2VideoEncoderFactory() {
  // Constrained Baseline profile, level 3.1, packetization mode 1.
  // This is the most widely compatible H.264 profile for WebRTC.
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));
}

V4L2VideoEncoderFactory::~V4L2VideoEncoderFactory() {}

bool V4L2VideoEncoderFactory::IsSupported() {
  std::string device = livekit_ffi::V4l2H264EncoderWrapper::FindEncoderDevice();
  if (device.empty()) {
    RTC_LOG(LS_INFO) << "V4L2: No H.264 M2M encoder device found.";
    return false;
  }
  RTC_LOG(LS_INFO) << "V4L2: H.264 M2M encoder is supported at " << device;
  return true;
}

std::unique_ptr<VideoEncoder> V4L2VideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      return std::make_unique<V4L2H264EncoderImpl>(env, format);
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> V4L2VideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> V4L2VideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
