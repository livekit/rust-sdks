#ifndef VIDEO_DECODER_FACTORY_H
#define VIDEO_DECODER_FACTORY_H

#include "api/video_codecs/video_decoder_factory.h"

namespace livekit {
class VideoDecoderFactory : public webrtc::VideoDecoderFactory {
 public:
  VideoDecoderFactory();

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  std::unique_ptr<webrtc::VideoDecoder> CreateVideoDecoder(
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoDecoderFactory>> factories_;
};
}  // namespace livekit

#endif  // VIDEO_DECODER_FACTORY_H
