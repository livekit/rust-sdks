/*
 * Placeholder V4L2 H.264 encoder implementation.
 *
 * This currently delegates all work to an internal software H.264 encoder.
 * A real V4L2 M2M backend can replace the delegation in a follow-up step.
 */

#ifndef WEBRTC_V4L2_H264_ENCODER_IMPL_H_
#define WEBRTC_V4L2_H264_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"

#if defined(__linux__)
#include <linux/videodev2.h>
#endif

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

#if defined(__linux__)
  struct V4L2PlaneBuffer {
    void* start = nullptr;
    size_t length = 0;
  };

  struct V4L2Buffer {
    std::vector<V4L2PlaneBuffer> planes;
  };

  int fd_ = -1;
  bool v4l2_initialized_ = false;
  uint32_t width_ = 0;
  uint32_t height_ = 0;
  std::vector<V4L2Buffer> output_buffers_;
  std::vector<V4L2Buffer> capture_buffers_;

  int InitV4L2Device(const VideoCodec* codec_settings);
  int EncodeWithV4L2(const VideoFrame& frame,
                     const std::vector<VideoFrameType>* frame_types);
  int DrainEncodedFrame(EncodedImage& encoded_image);
  void CleanupV4L2();
#endif

  // Software fallback encoder (OpenH264, etc.) used when V4L2 is unavailable or fails.
  std::unique_ptr<VideoEncoder> fallback_encoder_;

  // Callback used by both the V4L2 and fallback paths.
  EncodedImageCallback* encoded_image_callback_ = nullptr;
};

}  // namespace webrtc

#endif  // WEBRTC_V4L2_H264_ENCODER_IMPL_H_


