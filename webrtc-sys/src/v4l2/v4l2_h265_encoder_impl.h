#ifndef WEBRTC_V4L2_H265_ENCODER_IMPL_H_
#define WEBRTC_V4L2_H265_ENCODER_IMPL_H_

#include <linux/videodev2.h>
#include <memory>
#include <vector>
#include <string>

#include "absl/container/inlined_vector.h"
#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/transport/rtp/dependency_descriptor.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/video_encoder.h"

namespace webrtc {

class V4L2H265EncoderImpl : public VideoEncoder {
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
  V4L2H265EncoderImpl(const webrtc::Environment& env,
                      const std::string& device_path,
                      const SdpVideoFormat& format);
  ~V4L2H265EncoderImpl() override;

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
  // V4L2 initialization and cleanup
  bool InitializeV4L2Device();
  void CleanupV4L2Device();
  
  // Buffer management
  bool AllocateInputBuffers();
  bool AllocateOutputBuffers();
  void DeallocateBuffers();
  
  // Encoding operations
  bool EncodeFrame(const VideoFrame& frame, bool is_keyframe);
  int32_t ProcessEncodedFrame(std::vector<uint8_t>& packet,
                              const VideoFrame& inputFrame);

 private:
  const webrtc::Environment& env_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;

  std::string device_path_;
  int device_fd_ = -1;
  
  // V4L2 specific structures
  struct v4l2_format output_format_;  // Input (raw YUV)
  struct v4l2_format capture_format_; // Output (encoded)
  
  std::vector<void*> input_buffers_;
  std::vector<size_t> input_buffer_sizes_;
  std::vector<void*> output_buffers_;
  std::vector<size_t> output_buffer_sizes_;
  
  uint32_t num_input_buffers_ = 0;
  uint32_t num_output_buffers_ = 0;

  LayerConfig configuration_;
  EncodedImage encoded_image_;
  VideoCodec codec_;
  
  void ReportInit();
  void ReportError();
  bool has_reported_init_ = false;
  bool has_reported_error_ = false;
  const SdpVideoFormat format_;
  bool current_encoding_is_keyframe_ = false;
  
  bool encoder_initialized_ = false;
  uint64_t frame_count_ = 0;
};

}  // namespace webrtc

#endif  // WEBRTC_V4L2_H265_ENCODER_IMPL_H_

