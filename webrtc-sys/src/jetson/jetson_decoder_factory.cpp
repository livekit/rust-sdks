#include "jetson_decoder_factory.h"

#include <memory>

#include "jetson_h264_decoder.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "rtc_base/logging.h"

namespace webrtc {

namespace {

constexpr char kSdpKeyNameCodecImpl[] = "implementation_name";
constexpr char kCodecName[] = "JetsonV4L2";

std::vector<SdpVideoFormat> SupportedJetsonDecoderCodecs() {
  std::vector<SdpVideoFormat> formats = {
      CreateH264Format(webrtc::H264Profile::kProfileConstrainedBaseline,
                       webrtc::H264Level::kLevel5_1, "1"),
      CreateH264Format(webrtc::H264Profile::kProfileBaseline,
                       webrtc::H264Level::kLevel5_1, "1"),
      CreateH264Format(webrtc::H264Profile::kProfileMain,
                       webrtc::H264Level::kLevel5_1, "1"),
      CreateH264Format(webrtc::H264Profile::kProfileHigh,
                       webrtc::H264Level::kLevel5_1, "1"),
  };

  for (auto& format : formats) {
    format.parameters.emplace(kSdpKeyNameCodecImpl, kCodecName);
  }
  return formats;
}

}  // namespace

JetsonVideoDecoderFactory::JetsonVideoDecoderFactory()
    : supported_formats_(SupportedJetsonDecoderCodecs()) {
  RTC_LOG(LS_INFO) << "JetsonVideoDecoderFactory created with "
                   << supported_formats_.size() << " supported formats.";
}

JetsonVideoDecoderFactory::~JetsonVideoDecoderFactory() = default;

bool JetsonVideoDecoderFactory::IsSupported() {
  RTC_LOG(LS_INFO)
      << "Jetson V4L2 hardware decoder support is compiled in.";
  return true;
}

std::vector<SdpVideoFormat> JetsonVideoDecoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::unique_ptr<VideoDecoder> JetsonVideoDecoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      RTC_LOG(LS_INFO) << "Using Jetson V4L2 hardware decoder for H264";
      return std::make_unique<JetsonH264Decoder>();
    }
  }
  return nullptr;
}

}  // namespace webrtc
