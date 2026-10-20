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

#include "jetson_decoder_factory.h"

#include <cstdlib>

#include "h265_decoder_impl.h"
#include "jetson_mmapi_decoder.h"
#include "rtc_base/logging.h"

namespace webrtc {

namespace {

constexpr char kDisableJetsonDecoderEnvVar[] = "LK_DISABLE_JETSON_DEC";

bool IsH265(const SdpVideoFormat& format) {
  return format.name == "H265" || format.name == "HEVC";
}

}  // namespace

JetsonVideoDecoderFactory::JetsonVideoDecoderFactory() {
  supported_formats_.push_back(SdpVideoFormat("H265"));
  supported_formats_.push_back(SdpVideoFormat("HEVC"));
}

JetsonVideoDecoderFactory::~JetsonVideoDecoderFactory() = default;

bool JetsonVideoDecoderFactory::IsSupported() {
  if (std::getenv(kDisableJetsonDecoderEnvVar) != nullptr) {
    RTC_LOG(LS_INFO) << "Jetson hardware decoder disabled because "
                     << kDisableJetsonDecoderEnvVar << " is set.";
    return false;
  }
  if (!livekit::JetsonMmapiDecoder::IsCodecSupported(
          livekit::JetsonCodec::kH265)) {
    RTC_LOG(LS_INFO) << "Jetson MMAPI H265 decoder is not available.";
    return false;
  }
  RTC_LOG(LS_INFO) << "Jetson MMAPI H265 decoder is supported.";
  return true;
}

std::vector<SdpVideoFormat> JetsonVideoDecoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::unique_ptr<VideoDecoder> JetsonVideoDecoderFactory::Create(
    const Environment& /*env*/,
    const SdpVideoFormat& format) {
  if (!IsH265(format)) {
    return nullptr;
  }
  RTC_LOG(LS_INFO) << "Using Jetson MMAPI HW decoder for H265";
  return std::make_unique<JetsonH265DecoderImpl>();
}

}  // namespace webrtc
