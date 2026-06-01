#ifndef WEBRTC_JETSON_H264_DECODER_H_
#define WEBRTC_JETSON_H264_DECODER_H_

#include <memory>
#include <queue>
#include <vector>

#include <linux/videodev2.h>

#include "NvBuffer.h"
#include "NvVideoDecoder.h"
#include "api/video_codecs/video_decoder.h"
#include "common_video/h264/h264_bitstream_parser.h"
#include "common_video/h264/pps_parser.h"
#include "common_video/h264/sps_parser.h"

namespace webrtc {

class JetsonH264Decoder : public VideoDecoder {
 public:
  JetsonH264Decoder();
  JetsonH264Decoder(const JetsonH264Decoder&) = delete;
  JetsonH264Decoder& operator=(const JetsonH264Decoder&) = delete;
  ~JetsonH264Decoder() override;

  bool Configure(const Settings& settings) override;
  int32_t Decode(const EncodedImage& input_image,
                 bool missing_frames,
                 int64_t render_time_ms) override;
  int32_t RegisterDecodeCompleteCallback(
      DecodedImageCallback* callback) override;
  int32_t Release() override;
  DecoderInfo GetDecoderInfo() const override;

 private:
  class State;

  bool ConfigureCapturePlane();
  bool PollResolutionChange();
  bool QueueInputBuffer(const EncodedImage& input_image);
  void DrainOutputPlane();
  std::vector<VideoFrame> DrainCapturePlane(uint32_t fallback_rtp_timestamp,
                                            const ColorSpace& color_space);

  Settings settings_;
  std::shared_ptr<State> state_;
  DecodedImageCallback* decoded_complete_callback_ = nullptr;
  std::queue<uint32_t> free_output_buffers_;
  H264BitstreamParser h264_bitstream_parser_;
};

}  // namespace webrtc

#endif  // WEBRTC_JETSON_H264_DECODER_H_
