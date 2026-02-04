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

#ifndef LIVEKIT_RTC_PASSTHROUGH_ENCODER_H
#define LIVEKIT_RTC_PASSTHROUGH_ENCODER_H

#include <memory>
#include <optional>
#include <vector>

#include "api/video/encoded_image.h"
#include "api/video_codecs/video_encoder.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit_ffi {

class EncodedFrameProvider;

// Holds pre-encoded frame data
struct PreEncodedFrame {
  rtc::scoped_refptr<webrtc::EncodedImageBufferInterface> data;
  int64_t capture_time_us;
  uint32_t rtp_timestamp;
  uint32_t width;
  uint32_t height;
  bool is_keyframe;
  bool has_sps_pps;  // H264: includes SPS/PPS NALUs
};

// A passthrough encoder that emits pre-encoded frames without re-encoding.
// Used with EncodedVideoSource to inject pre-encoded H264/VP8/etc frames.
// The encoder uses the frame ID to look up the EncodedFrameProvider from
// the EncodedVideoSourceRegistry at encode time.
class PassthroughVideoEncoder : public webrtc::VideoEncoder {
 public:
  explicit PassthroughVideoEncoder(webrtc::VideoCodecType codec_type);
  ~PassthroughVideoEncoder() override;

  // VideoEncoder interface
  void SetFecControllerOverride(
      webrtc::FecControllerOverride* fec_controller_override) override;
  int InitEncode(const webrtc::VideoCodec* codec_settings,
                 const Settings& settings) override;
  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(const webrtc::VideoFrame& frame,
                 const std::vector<webrtc::VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  void OnPacketLossRateUpdate(float packet_loss_rate) override;
  void OnRttUpdate(int64_t rtt_ms) override;
  void OnLossNotification(const LossNotification& loss_notification) override;
  EncoderInfo GetEncoderInfo() const override;

 private:
  webrtc::VideoCodecType codec_type_;
  webrtc::EncodedImageCallback* callback_ = nullptr;

  mutable webrtc::Mutex mutex_;
  uint32_t configured_width_ = 0;
  uint32_t configured_height_ = 0;
  uint32_t target_bitrate_bps_ = 0;
  uint32_t max_framerate_ = 0;
  bool initialized_ = false;
};

}  // namespace livekit_ffi

#endif  // LIVEKIT_RTC_PASSTHROUGH_ENCODER_H
