#ifndef WEBRTC_JETSON_H265_ENCODER_IMPL_H_
#define WEBRTC_JETSON_H265_ENCODER_IMPL_H_

#include <cstdint>
#include <memory>
#include <vector>

#include "absl/container/inlined_vector.h"
#include "api/environment/environment.h"
#include "api/transport/rtp/dependency_descriptor.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/video_encoder.h"
#include "modules/video_coding/svc/scalable_video_controller.h"
#include "modules/video_coding/utility/quality_scaler.h"

#include "jetson_mmapi_encoder.h"

namespace webrtc {

class JetsonH265EncoderImpl : public VideoEncoder {
 public:
  struct LayerConfig {
    int simulcast_idx = 0;
    int width = -1;
    int height = -1;
    bool sending = true;
    bool key_frame_request = false;
    float max_frame_rate = 0;
    uint32_t target_bps = 0;
    uint32_t max_bps = 0;
    bool frame_dropping_on = false;
    int key_frame_interval = 0;
    int num_temporal_layers = 1;

    void SetStreamState(bool send_stream);
  };

 public:
  JetsonH265EncoderImpl(const webrtc::Environment& env,
                        const SdpVideoFormat& format);
  ~JetsonH265EncoderImpl() override;

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
  int32_t ProcessEncodedFrame(std::vector<uint8_t>& packet,
                              const ::webrtc::VideoFrame& input_frame,
                              bool is_keyframe);

  const webrtc::Environment& env_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;
  livekit::JetsonMmapiEncoder encoder_;
  LayerConfig configuration_;
  EncodedImage encoded_image_;
  VideoCodec codec_;
  void ReportInit();
  void ReportError();
  bool has_reported_init_ = false;
  bool has_reported_error_ = false;
  const SdpVideoFormat format_;
  std::vector<uint8_t> nv12_buffer_;
};

}  // namespace webrtc

#endif  // WEBRTC_JETSON_H265_ENCODER_IMPL_H_
