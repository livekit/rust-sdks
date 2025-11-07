#ifndef WEBRTC_JETSON_VIDEO_ENCODER_FACTORY_H_
#define WEBRTC_JETSON_VIDEO_ENCODER_FACTORY_H_

#include <memory>
#include <optional>
#include <vector>

#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace webrtc {

// Jetson (NVIDIA Tegra) hardware video encoder factory using V4L2 M2M encoders.
// This factory is intended for Linux aarch64 Jetson devices (Orin/Thor).
class JetsonVideoEncoderFactory : public VideoEncoderFactory {
 public:
  JetsonVideoEncoderFactory();
  ~JetsonVideoEncoderFactory() override = default;

  static bool IsSupported();

  // webrtc::VideoEncoderFactory
  std::vector<SdpVideoFormat> GetSupportedFormats() const override;
  std::unique_ptr<VideoEncoder> Create(const Environment& env,
                                       const SdpVideoFormat& format) override;

  std::vector<SdpVideoFormat> GetImplementations() const;

 private:
  std::vector<SdpVideoFormat> supported_formats_;
};

}  // namespace webrtc

#endif  // WEBRTC_JETSON_VIDEO_ENCODER_FACTORY_H_


