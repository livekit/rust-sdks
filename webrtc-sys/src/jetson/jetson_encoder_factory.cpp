#include "jetson_encoder_factory.h"

#include <cstdlib>
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

namespace {
// Diagnostic gate: when LK_DISABLE_JETSON_AV1 is set, the Jetson factory does
// not advertise or create an AV1 encoder, so the builtin libaom AV1 software
// encoder is used instead. Used to bisect encoder-side vs pipeline-side issues.
bool JetsonAv1Disabled() {
  return std::getenv("LK_DISABLE_JETSON_AV1") != nullptr;
}
}  // namespace

JetsonVideoEncoderFactory::JetsonVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  supported_formats_.push_back(SdpVideoFormat("H265"));
  supported_formats_.push_back(SdpVideoFormat("HEVC"));

  if (!JetsonAv1Disabled() && livekit::JetsonMmapiEncoder::IsCodecSupported(
          livekit::JetsonCodec::kAV1)) {
    absl::InlinedVector<ScalabilityMode, kScalabilityModeCount>
        scalability_modes;
    scalability_modes.push_back(ScalabilityMode::kL1T1);
    supported_formats_.push_back(
        SdpVideoFormat(SdpVideoFormat::AV1Profile0(), scalability_modes));
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

    if (format.name == "AV1" && !JetsonAv1Disabled()) {
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
