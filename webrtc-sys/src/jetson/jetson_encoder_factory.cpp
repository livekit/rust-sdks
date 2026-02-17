#include "jetson_encoder_factory.h"

#include <iostream>
#include <map>
#include <memory>

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
