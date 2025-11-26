#include "v4l2/v4l2_encoder_factory.h"

#include <memory>

#include "rtc_base/logging.h"
#include "v4l2/h264_encoder_impl.h"

namespace webrtc {

V4L2VideoEncoderFactory::V4L2VideoEncoderFactory() {
  // Only advertise baseline H.264 for now.
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));
}

V4L2VideoEncoderFactory::~V4L2VideoEncoderFactory() = default;

bool V4L2VideoEncoderFactory::IsSupported() {
#if defined(__linux__) && (defined(__aarch64__) || defined(__arm__))
  // On Linux/ARM we assume that a V4L2-backed encoder is desirable.
  // A more robust implementation could probe for specific M2M encoder nodes.
  RTC_LOG(LS_INFO) << "V4L2VideoEncoderFactory considered supported on Linux/ARM";
  return true;
#else
  return false;
#endif
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


