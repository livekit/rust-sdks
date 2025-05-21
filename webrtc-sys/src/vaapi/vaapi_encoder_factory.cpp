#include "vaapi_encoder_factory.h"
#include "h264_encoder_impl.h"

namespace webrtc {

VAAPIVideoEncoderFactory::VAAPIVideoEncoderFactory() {
  supported_formats_.push_back(SdpVideoFormat("H264"));
  implementations_.push_back(SdpVideoFormat("H264"));
}

VAAPIVideoEncoderFactory::~VAAPIVideoEncoderFactory() {

}
std::unique_ptr<VideoEncoder> VAAPIVideoEncoderFactory::Create(
    const Environment& env, const SdpVideoFormat& format) {
  if (format.IsSameCodec(supported_formats_[0])) {
    return std::make_unique<VAAPIH264EncoderWrapper>(env, std::make_unique<livekit::VaapiEncoderWrapper>());
  }
  return nullptr;
}
std::vector<SdpVideoFormat> VAAPIVideoEncoderFactory::GetSupportedFormats() const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> VAAPIVideoEncoderFactory::GetImplementations() const {
  return implementations_;
}

}  // namespace webrtc

