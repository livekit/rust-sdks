/*
 * V4L2-backed VideoEncoderFactory scaffold.
 *
 * NOTE: This currently delegates to the software H.264 encoder internally.
 * A true V4L2 M2M implementation can be wired into V4L2H264EncoderImpl later.
 */

#ifndef V4L2_VIDEO_ENCODER_FACTORY_H_
#define V4L2_VIDEO_ENCODER_FACTORY_H_

#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace webrtc {

class V4L2H264EncoderImpl;

class V4L2VideoEncoderFactory : public VideoEncoderFactory {
 public:
  V4L2VideoEncoderFactory();
  ~V4L2VideoEncoderFactory() override;

  // Returns true if this platform should attempt to use a V4L2-backed encoder.
  static bool IsSupported();

  std::unique_ptr<VideoEncoder> Create(const Environment& env,
                                       const SdpVideoFormat& format) override;

  // Returns a list of supported codecs in order of preference.
  std::vector<SdpVideoFormat> GetSupportedFormats() const override;

  std::vector<SdpVideoFormat> GetImplementations() const override;

  std::unique_ptr<EncoderSelectorInterface> GetEncoderSelector() const override {
    return nullptr;
  }

 private:
  std::vector<SdpVideoFormat> supported_formats_;
};

}  // namespace webrtc

#endif  // V4L2_VIDEO_ENCODER_FACTORY_H_


