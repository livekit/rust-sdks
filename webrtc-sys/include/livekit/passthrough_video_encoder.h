/*
 * Copyright 2026 LiveKit, Inc.
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

#pragma once

#include <cstdint>
#include <functional>
#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video/video_frame.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_codec.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "livekit/encoded_video_source.h"

namespace livekit_ffi {

// Encoder that takes pre-encoded bitstream bytes from a paired
// EncodedVideoTrackSource and forwards them unmodified to the
// EncodedImageCallback. Used for applications that already produce H.264 /
// H.265 / VP8 / VP9 / AV1 bitstreams (e.g. from a hardware capturer or a
// remote camera feed) and want to pipe them through WebRTC without
// re-encoding.
class PassthroughVideoEncoder : public webrtc::VideoEncoder {
 public:
  // The encoder holds a strong ref to the source so that:
  //   * Encode() can pop frames / notify keyframe requests without a registry
  //     lookup (bound 1:1 at construction)
  //   * SetRates() can forward congestion-controller target bitrate updates
  //     to the Rust producer immediately.
  explicit PassthroughVideoEncoder(
      webrtc::scoped_refptr<EncodedVideoTrackSource::InternalSource> source);
  ~PassthroughVideoEncoder() override;

  // webrtc::VideoEncoder
  int InitEncode(const webrtc::VideoCodec* codec_settings,
                 const Settings& settings) override;
  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  EncoderInfo GetEncoderInfo() const override;

 private:
  const webrtc::scoped_refptr<EncodedVideoTrackSource::InternalSource> source_;
  const EncodedVideoCodecType codec_;
  webrtc::EncodedImageCallback* callback_ = nullptr;
  webrtc::VideoCodec codec_settings_{};
  bool initialized_ = false;
};

// Wraps a webrtc::VideoEncoder built lazily on the first Encode() call. This
// lets us delay the decision of "passthrough vs. real encoder" until we can
// inspect the incoming VideoFrame::id() and check the EncodedSourceRegistry.
//
// Cost: one registry lookup + one encoder construction on the first frame.
// Subsequent frames are a single virtual call with no extra overhead.
class LazyVideoEncoder : public webrtc::VideoEncoder {
 public:
  // `real_encoder_builder` is called at most once, the first time Encode()
  // receives a frame that does not correspond to an encoded source.
  using RealEncoderBuilder =
      std::function<std::unique_ptr<webrtc::VideoEncoder>()>;

  LazyVideoEncoder(webrtc::SdpVideoFormat format,
                   RealEncoderBuilder real_encoder_builder);
  ~LazyVideoEncoder() override;

  int InitEncode(const webrtc::VideoCodec* codec_settings,
                 const Settings& settings) override;
  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override;
  int32_t Release() override;
  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* frame_types) override;
  void SetRates(const RateControlParameters& parameters) override;
  void OnPacketLossRateUpdate(float packet_loss_rate) override;
  void OnRttUpdate(int64_t rtt_ms) override;
  void OnLossNotification(const LossNotification& loss_notification) override;
  EncoderInfo GetEncoderInfo() const override;

 private:
  // Build the underlying encoder based on frame.id() lookup. Returns true on
  // success. Safe to call exactly once.
  bool BuildInner(uint16_t frame_id);

  const webrtc::SdpVideoFormat format_;
  RealEncoderBuilder real_encoder_builder_;

  // Set on first Encode().
  std::unique_ptr<webrtc::VideoEncoder> inner_;
  bool is_passthrough_ = false;

  // Deferred InitEncode() args.
  webrtc::VideoCodec pending_codec_settings_{};
  webrtc::VideoEncoder::Settings pending_settings_{
      webrtc::VideoEncoder::Capabilities(/*loss_notification=*/false),
      /*number_of_cores=*/1,
      /*max_payload_size=*/1200};
  bool has_pending_init_ = false;
  webrtc::EncodedImageCallback* callback_ = nullptr;

  // Cached rate / loss / rtt updates that arrived before Encode().
  std::optional<webrtc::VideoEncoder::RateControlParameters> pending_rates_;
  std::optional<float> pending_loss_rate_;
  std::optional<int64_t> pending_rtt_ms_;
};

}  // namespace livekit_ffi
