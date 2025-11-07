#ifndef WEBRTC_JETSON_H264_ENCODER_IMPL_H_
#define WEBRTC_JETSON_H264_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include "api/video/video_frame.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/units/data_rate.h"
#include "rtc_base/logging.h"

namespace livekit { class V4L2H264Encoder; }

namespace webrtc {

class JetsonH264EncoderImpl : public VideoEncoder {
 public:
  explicit JetsonH264EncoderImpl(const SdpVideoFormat& format);
  ~JetsonH264EncoderImpl() override;

  // VideoEncoder
  int32_t InitEncode(const VideoCodec* codec_settings, const Settings& settings) override;
  int32_t RegisterEncodeCompleteCallback(EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const VideoFrame& frame, const std::vector<VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  EncoderInfo GetEncoderInfo() const override;

 private:
  const SdpVideoFormat format_;
  const VideoCodec* codec_ = nullptr;
  EncodedImageCallback* encoded_image_callback_ = nullptr;

  bool sending_ = false;
  bool keyframe_requested_ = false;

  std::unique_ptr<livekit::V4L2H264Encoder> v4l2_;
};

}  // namespace webrtc

#endif  // WEBRTC_JETSON_H264_ENCODER_IMPL_H_


