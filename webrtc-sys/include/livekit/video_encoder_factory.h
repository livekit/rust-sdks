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

#include <cstdint>
#include <memory>
#include <optional>
#include <vector>

#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace livekit_ffi {
enum class VideoEncoderBackend : std::int32_t;

struct VideoEncoderBackendFactory {
  VideoEncoderBackend backend;
  std::unique_ptr<webrtc::VideoEncoderFactory> factory;
};

class VideoEncoderFactory : public webrtc::VideoEncoderFactory {
  class InternalFactory : public webrtc::VideoEncoderFactory {
   public:
    InternalFactory();

    std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

    std::vector<webrtc::SdpVideoFormat> GetImplementations() const override;

    CodecSupport QueryCodecSupport(
        const webrtc::SdpVideoFormat& format,
        std::optional<std::string> scalability_mode) const override;

    std::unique_ptr<webrtc::VideoEncoder> Create(
        const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) override;

   private:
    std::vector<VideoEncoderBackendFactory> factories_;
  };

 public:
  VideoEncoderFactory();

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  std::vector<webrtc::SdpVideoFormat> GetImplementations() const override;

  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat& format,
      std::optional<std::string> scalability_mode) const override;

  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) override;

 private:
  std::unique_ptr<InternalFactory> internal_factory_;
};
}  // namespace livekit_ffi
