#include "livekit/video_decoder_factory.h"

#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "livekit/objc_video_factory.h"
#include "media/base/media_constants.h"
#include "rtc_base/logging.h"

namespace livekit {

VideoDecoderFactory::VideoDecoderFactory() {
  factories_.push_back(webrtc::CreateBuiltinVideoDecoderFactory());

#ifdef __APPLE__
  factories_.push_back(livekit::CreateObjCVideoDecoderFactory());
#endif

  // TODO(theomonnom): Add other HW decoders here
}

std::vector<webrtc::SdpVideoFormat> VideoDecoderFactory::GetSupportedFormats()
    const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& factory : factories_) {
    auto supported_formats = factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }
  return formats;
}

std::unique_ptr<webrtc::VideoDecoder> VideoDecoderFactory::CreateVideoDecoder(
    const webrtc::SdpVideoFormat& format) {
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format))
        return factory->CreateVideoDecoder(format);
    }
  }

  RTC_LOG(LS_ERROR) << "No VideoDecoder found for " << format.name;
  return nullptr;
}

}  // namespace livekit
