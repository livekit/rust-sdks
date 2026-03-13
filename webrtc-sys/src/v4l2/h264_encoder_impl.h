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

#ifndef V4L2_H264_ENCODER_IMPL_H_
#define V4L2_H264_ENCODER_IMPL_H_

#include <memory>
#include <vector>

#include "api/video/i420_buffer.h"
#include "api/video_codecs/video_encoder.h"
#include "common_video/h264/h264_bitstream_parser.h"
#include "modules/video_coding/codecs/h264/include/h264.h"

#include "v4l2_h264_encoder_wrapper.h"

namespace webrtc {

// WebRTC VideoEncoder implementation backed by a V4L2 M2M H.264 hardware
// encoder (e.g. the bcm2835-codec on Raspberry Pi 4).
//
// This class bridges the WebRTC encoding interface with the low-level
// V4l2H264EncoderWrapper.  It handles codec configuration, rate control
// callbacks, bitstream parsing (for QP extraction), and delivery of
// encoded images to the WebRTC pipeline.
//
// Simulcast is not supported -- only a single spatial/temporal layer.
class V4L2H264EncoderImpl : public VideoEncoder {
 public:
  // Per-layer encoding configuration (single layer only for V4L2).
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

    // Toggle the stream on/off.  Transitioning to |send_stream=true|
    // automatically requests a keyframe so the receiver can resync.
    void SetStreamState(bool send_stream);
  };

  explicit V4L2H264EncoderImpl(const webrtc::Environment& env,
                                const SdpVideoFormat& format);
  ~V4L2H264EncoderImpl() override;

  // --- VideoEncoder interface ---
  int32_t InitEncode(const VideoCodec* codec_settings,
                     const Settings& settings) override;
  int32_t RegisterEncodeCompleteCallback(
      EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const VideoFrame& frame,
                 const std::vector<VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  EncoderInfo GetEncoderInfo() const override;

 private:
  // One-shot histogram reporting helpers.
  void ReportInit();
  void ReportError();

  const webrtc::Environment& env_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;

  // The underlying V4L2 hardware encoder.
  std::unique_ptr<livekit_ffi::V4l2H264EncoderWrapper> encoder_;

  LayerConfig configuration_;
  EncodedImage encoded_image_;
  H264PacketizationMode packetization_mode_;
  VideoCodec codec_;

  // Histogram dedup flags.
  bool has_reported_init_ = false;
  bool has_reported_error_ = false;

  // Used to extract QP from the encoded bitstream.
  webrtc::H264BitstreamParser h264_bitstream_parser_;

  const SdpVideoFormat format_;
};

}  // namespace webrtc

#endif  // V4L2_H264_ENCODER_IMPL_H_
