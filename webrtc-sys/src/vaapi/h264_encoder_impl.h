#ifndef VAAPI_H264_ENCODER_IMPL_H_
#define VAAPI_H264_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include "absl/container/inlined_vector.h"
#include "api/transport/rtp/dependency_descriptor.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/video_encoder.h"
#include "common_video/h264/h264_bitstream_parser.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "modules/video_coding/svc/scalable_video_controller.h"
#include "modules/video_coding/utility/quality_scaler.h"
#include "vaapi_encoder.h"

namespace webrtc {

class VAAPIH264EncoderWrapper : public VideoEncoder {
 public:
  VAAPIH264EncoderWrapper(
      std::unique_ptr<livekit::VaapiEncoderWrapper> vaapi_encoder);
  ~VAAPIH264EncoderWrapper() override;

  int32_t InitEncode(const VideoCodec* codec_settings,
                     const Settings& settings) override;

  int32_t RegisterEncodeCompleteCallback(
      EncodedImageCallback* callback) override;

  int32_t Release() override;

  int32_t Encode(const VideoFrame& frame,
                 const std::vector<VideoFrameType>* frame_types) override;

  void SetRates(const RateControlParameters& rc_parameters) override;

  EncoderInfo GetEncoderInfo() const { return encoder_info_; }

 private:
  EncoderInfo encoder_info_;
  EncodedImageCallback* callback_;
  std::unique_ptr<livekit::VaapiEncoderWrapper> encoder_;
};

}  // namespace webrtc

#endif  // VAAPI_H264_ENCODER_IMPL_H_
