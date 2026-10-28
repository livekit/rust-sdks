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

#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory.h"

#ifdef LIVEKIT_TEST
#include "rust/cxx.h"
#endif

namespace livekit_ffi {

class PassthroughVideoEncoderFactory : public webrtc::VideoEncoderFactory {
 public:
  PassthroughVideoEncoderFactory();
  ~PassthroughVideoEncoderFactory() override = default;

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;
  std::vector<webrtc::SdpVideoFormat> GetImplementations() const override;
  CodecSupport QueryCodecSupport(
      const webrtc::SdpVideoFormat& format,
      std::optional<std::string> scalability_mode) const override;
  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& env,
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<webrtc::SdpVideoFormat> supported_formats_;
};

#ifdef LIVEKIT_TEST
// Frame kinds for passthrough_chain_guard_for_test().
constexpr uint8_t kChainTestKey = 0;
constexpr uint8_t kChainTestDelta = 1;
constexpr uint8_t kChainTestDroppedBeforeEncoder = 2;
// Result bits, one result per frame that reached the encoder.
constexpr uint8_t kChainTestSent = 1;
constexpr uint8_t kChainTestKeyframeRequested = 2;

// Feeds a frame sequence through a PassthroughVideoEncoder.
rust::Vec<uint8_t> passthrough_chain_guard_for_test(
    rust::Slice<const uint8_t> frames);
#endif

}  // namespace livekit_ffi
