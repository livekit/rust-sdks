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

#pragma once

#include <memory>
#include <mutex>
#include <unordered_map>

#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"
#include "livekit/encoded_video_source.h"

namespace livekit_ffi {

/// A video encoder that passes through pre-encoded frame data.
///
/// Instead of actually encoding the incoming raw frame, it pulls the
/// next queued `EncodedFrameData` from the associated
/// `EncodedVideoTrackSource` and delivers it to the WebRTC RTP pipeline
/// via `EncodedImageCallback::OnEncodedImage()`.
class PassthroughVideoEncoder : public webrtc::VideoEncoder {
 public:
  explicit PassthroughVideoEncoder(
      std::shared_ptr<EncodedVideoTrackSource> source);
  ~PassthroughVideoEncoder() override = default;

  int32_t InitEncode(const webrtc::VideoCodec* codec_settings,
                     const webrtc::VideoEncoder::Settings& settings) override;

  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override;

  int32_t Release() override;

  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* frame_types) override;

  void SetRates(const RateControlParameters& parameters) override;

  EncoderInfo GetEncoderInfo() const override;

 private:
  std::shared_ptr<EncodedVideoTrackSource> source_;
  webrtc::EncodedImageCallback* callback_ = nullptr;
  webrtc::VideoCodec codec_;
  bool sending_ = false;
  uint32_t simulcast_index_ = 0;
};

/// A minimal VideoEncoderFactory that only produces PassthroughVideoEncoder
/// instances for a given EncodedVideoTrackSource.  Used as the inner factory
/// inside SimulcastEncoderAdapter so that each simulcast layer gets its own
/// PassthroughVideoEncoder pulling from the correct per-layer queue.
class PassthroughVideoEncoderFactory : public webrtc::VideoEncoderFactory {
 public:
  explicit PassthroughVideoEncoderFactory(
      std::shared_ptr<EncodedVideoTrackSource> source,
      const webrtc::SdpVideoFormat& format);

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& env,
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::shared_ptr<EncodedVideoTrackSource> source_;
  webrtc::SdpVideoFormat format_;
};

/// Global registry that maps source pointers to their shared_ptr so the
/// encoder factory can look them up when creating an encoder.
class EncodedSourceRegistry {
 public:
  static EncodedSourceRegistry& instance();

  void register_source(
      const webrtc::VideoTrackSourceInterface* key,
      std::shared_ptr<EncodedVideoTrackSource> source);
  void unregister_source(const webrtc::VideoTrackSourceInterface* key);

  std::shared_ptr<EncodedVideoTrackSource> find(
      const webrtc::VideoTrackSourceInterface* key) const;

  /// Find a registered encoded source whose codec matches the given SDP
  /// codec name (e.g. "H264", "VP8").  Returns the first match or nullptr.
  std::shared_ptr<EncodedVideoTrackSource> find_by_codec_name(
      const std::string& codec_name) const;

 private:
  mutable std::mutex mutex_;
  std::unordered_map<const webrtc::VideoTrackSourceInterface*,
                     std::shared_ptr<EncodedVideoTrackSource>>
      sources_;
};

}  // namespace livekit_ffi
