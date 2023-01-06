//
// Created by Théo Monnom on 30/08/2022.
//

#ifndef VIDEO_DECODER_FACTORY_H
#define VIDEO_DECODER_FACTORY_H

#include "api/video_codecs/video_decoder_factory.h"
namespace livekit {
class VideoDecoderFactory : webrtc::VideoDecoderFactory {
 public:
  VideoDecoderFactory();
  ~VideoDecoderFactory() override = default;

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  std::unique_ptr<webrtc::VideoDecoder> CreateVideoDecoder(
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoDecoderFactory>> factories_;
};
}  // namespace livekit

#endif  // VIDEO_DECODER_FACTORY_H
