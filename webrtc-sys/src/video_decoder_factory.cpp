#include "livekit/video_decoder_factory.h"

#include "api/video_codecs/builtin_video_decoder_factory.h"
#include "api/video_codecs/builtin_video_encoder_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "media/base/media_constants.h"

namespace livekit {

VideoDecoderFactory::VideoDecoderFactory() {
  factories_.push_back(webrtc::CreateBuiltinVideoDecoderFactory());
}

std::vector<webrtc::SdpVideoFormat> VideoDecoderFactory::GetSupportedFormats()
    const {}

}  // namespace livekit
