#pragma once

#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace livekit {
class VideoEncoderFactory : public webrtc::VideoEncoderFactory {
 public:
  VideoEncoderFactory();

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  std::unique_ptr<webrtc::VideoEncoder> CreateVideoEncoder(
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoEncoderFactory>> factories_;
};
}  // namespace livekit
