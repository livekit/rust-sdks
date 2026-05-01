#ifndef VAAPI_H265_ENCODER_IMPL_H_
#define VAAPI_H265_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"

#include "vaapi_h265_encoder_wrapper.h"

namespace webrtc {

class VAAPIH265EncoderWrapper : public VideoEncoder {
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
  VAAPIH265EncoderWrapper(const webrtc::Environment& env,
                          const SdpVideoFormat& format);
  ~VAAPIH265EncoderWrapper() override;

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
  EncodedImageCallback* encoded_image_callback_ = nullptr;
  std::unique_ptr<livekit_ffi::VaapiH265EncoderWrapper> encoder_;
  LayerConfig configuration_;
  EncodedImage encoded_image_;
  VideoCodec codec_;
  void ReportInit();
  void ReportError();
  bool has_reported_init_ = false;
  bool has_reported_error_ = false;
  const SdpVideoFormat format_;
};

}  // namespace webrtc

#endif  // VAAPI_H265_ENCODER_IMPL_H_
