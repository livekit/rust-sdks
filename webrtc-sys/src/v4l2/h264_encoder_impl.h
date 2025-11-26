/*
 * Placeholder V4L2 H.264 encoder implementation.
 *
 * This currently delegates all work to an internal software H.264 encoder.
 * A real V4L2 M2M backend can replace the delegation in a follow-up step.
 */

#ifndef WEBRTC_V4L2_H264_ENCODER_IMPL_H_
#define WEBRTC_V4L2_H264_ENCODER_IMPL_H_

#include <memory>

#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/sdp_video_format.h"

namespace webrtc {

class V4L2H264EncoderImpl : public VideoEncoder {
 public:
  V4L2H264EncoderImpl(const webrtc::Environment& env,
                      const SdpVideoFormat& format);
  ~V4L2H264EncoderImpl() override;

  int32_t InitEncode(const VideoCodec* codec_settings,
                     const Settings& settings) override;

  int32_t RegisterEncodeCompleteCallback(
      EncodedImageCallback* callback) override;

  int32_t Release() override;

  int32_t Encode(const VideoFrame& frame,
                 const std::vector<VideoFrameType>* frame_types) override;

  void SetRates(const RateControlParameters& rc_parameters) override;

  EncoderInfo GetEncoderInfo() const override;

 private:
  const webrtc::Environment& env_;
  SdpVideoFormat format_;
  std::unique_ptr<webrtc::VideoEncoder> fallback_encoder_;
};

}  // namespace webrtc

#endif  // WEBRTC_V4L2_H264_ENCODER_IMPL_H_


