/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#ifndef MPP_H264_ENCODER_IMPL_H_
#define MPP_H264_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include <rockchip/rk_mpi.h>
#include <rockchip/mpp_buffer.h>
#include <rockchip/mpp_frame.h>
#include <rockchip/mpp_packet.h>
#include <rockchip/rk_venc_cfg.h>

#include "api/video/i420_buffer.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/video_encoder.h"
#include "common_video/h264/h264_bitstream_parser.h"
#include "mpp_encoder_session.h"
#include "modules/video_coding/codecs/h264/include/h264.h"

namespace webrtc {

class MppH264EncoderImpl : public VideoEncoder {
 public:
  struct LayerConfig {
    bool sending = true;
    bool key_frame_request = false;
    float max_frame_rate = 0;
    uint32_t target_bps = 0;

    void SetStreamState(bool send_stream);
  };

 public:
  MppH264EncoderImpl(const webrtc::Environment& env,
                     const SdpVideoFormat& format);
  ~MppH264EncoderImpl() override;

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
  int32_t InitMpp();
  int32_t ConfigureMpp();
  int32_t ProcessEncodedPacket(MppPacket packet,
                               const VideoFrame& input_frame);

 private:
  const webrtc::Environment& env_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;

  MppEncoderSession session_;

  LayerConfig configuration_;
  EncodedImage encoded_image_;
  H264PacketizationMode packetization_mode_;
  VideoCodec codec_;

  H264BitstreamParser h264_bitstream_parser_;
  H264Profile profile_ = H264Profile::kProfileConstrainedBaseline;
  H264Level level_ = H264Level::kLevel3_1;

  // Frame dimensions with stride alignment for MPP
  int hor_stride_ = 0;
  int ver_stride_ = 0;
  size_t frame_size_ = 0;

  // Tracks the pixel format currently configured in MPP (prep:format).
  // Updated lazily when the input frame type changes (e.g. I420 vs NV12).
  MppFrameFormat configured_fmt_ = MPP_FMT_YUV420P;
};

}  // namespace webrtc

#endif  // MPP_H264_ENCODER_IMPL_H_
