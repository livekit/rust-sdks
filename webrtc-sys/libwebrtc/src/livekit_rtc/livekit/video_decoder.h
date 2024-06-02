#ifndef LIVEKIT_VIDEO_DECODER_H
#define LIVEKIT_VIDEO_DECODER_H

#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_decoder_factory.h"

namespace livekit {
class VideoDecoderFactory : public webrtc::VideoDecoderFactory {
 public:
  VideoDecoderFactory();

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
                                 bool reference_scaling) const override;

  std::unique_ptr<webrtc::VideoDecoder> CreateVideoDecoder(
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoDecoderFactory>> factories_;
};
}  // namespace livekit

#endif  // LIVEKIT_VIDEO_DECODER_H
