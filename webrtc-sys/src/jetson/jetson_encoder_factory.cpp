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

#include "jetson_encoder_factory.h"

#include <iostream>
#include <map>
#include <memory>

#include "absl/container/inlined_vector.h"
#include "av1_encoder_impl.h"
#include "api/video_codecs/scalability_mode.h"
#include "h264_encoder_impl.h"
#include "h265_encoder_impl.h"
#include "jetson_mmapi_encoder.h"
#include "rtc_base/logging.h"

namespace webrtc {

JetsonVideoEncoderFactory::JetsonVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  supported_formats_.push_back(SdpVideoFormat("H265"));
  supported_formats_.push_back(SdpVideoFormat("HEVC"));

  // AV1 encoding is only available on Orin-class hardware; probe instead of
  // advertising unconditionally so older devices (e.g. Xavier) fall back to
  // the software libaom encoder.
  if (livekit::JetsonMmapiEncoder::IsCodecSupported(
          livekit::JetsonCodec::kAV1)) {
    absl::InlinedVector<ScalabilityMode, kScalabilityModeCount>
        scalability_modes;
    scalability_modes.push_back(ScalabilityMode::kL1T1);
    supported_formats_.push_back(
        SdpVideoFormat(SdpVideoFormat::AV1Profile0(), scalability_modes));
    RTC_LOG(LS_INFO) << "Jetson MMAPI AV1 encoder available.";
    std::cout << "Jetson MMAPI AV1 encoder available." << std::endl;
  } else {
    RTC_LOG(LS_INFO)
        << "Jetson MMAPI AV1 encoder not supported on this device.";
    std::cout << "Jetson MMAPI AV1 encoder not supported on this device."
              << std::endl;
  }
}

JetsonVideoEncoderFactory::~JetsonVideoEncoderFactory() {}

bool JetsonVideoEncoderFactory::IsSupported() {
  if (!livekit::JetsonMmapiEncoder::IsSupported()) {
    RTC_LOG(LS_WARNING) << "Jetson MMAPI encoder is not available.";
    std::cout << "Jetson MMAPI encoder is not available." << std::endl;
    return false;
  }
  RTC_LOG(LS_INFO) << "Jetson MMAPI encoder is supported.";
  std::cout << "Jetson MMAPI encoder is supported." << std::endl;
  return true;
}

std::unique_ptr<VideoEncoder> JetsonVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (!format.IsSameCodec(supported_format)) {
      continue;
    }

    if (format.name == "H264") {
      RTC_LOG(LS_INFO) << "Using Jetson MMAPI encoder for H264";
      std::cout << "Using Jetson MMAPI encoder for H264" << std::endl;
      return std::make_unique<JetsonH264EncoderImpl>(env, format);
    }

    if (format.name == "H265" || format.name == "HEVC") {
      RTC_LOG(LS_INFO) << "Using Jetson MMAPI encoder for H265/HEVC";
      std::cout << "Using Jetson MMAPI encoder for H265/HEVC" << std::endl;
      return std::make_unique<JetsonH265EncoderImpl>(env, format);
    }

    if (format.name == "AV1") {
      RTC_LOG(LS_INFO) << "Using Jetson MMAPI encoder for AV1";
      std::cout << "Using Jetson MMAPI encoder for AV1" << std::endl;
      return std::make_unique<JetsonAV1EncoderImpl>(env, format);
    }
  }
  return nullptr;
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> JetsonVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
