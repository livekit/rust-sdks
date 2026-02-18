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

#ifndef V4L2_VIDEO_ENCODER_FACTORY_H_
#define V4L2_VIDEO_ENCODER_FACTORY_H_

#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace webrtc {

// VideoEncoderFactory that creates V4L2-backed H.264 hardware encoders.
//
// On construction the factory advertises Constrained Baseline profile
// (the most widely compatible H.264 profile for WebRTC).  Call
// IsSupported() to check whether the current system actually has a
// suitable V4L2 M2M encoder device before registering this factory.
class V4L2VideoEncoderFactory : public VideoEncoderFactory {
 public:
  V4L2VideoEncoderFactory();
  ~V4L2VideoEncoderFactory() override;

  // Probe the system for a V4L2 M2M H.264 encoder device.
  static bool IsSupported();

  // --- VideoEncoderFactory interface ---
  std::unique_ptr<VideoEncoder> Create(const Environment& env,
                                       const SdpVideoFormat& format) override;
  std::vector<SdpVideoFormat> GetSupportedFormats() const override;
  std::vector<SdpVideoFormat> GetImplementations() const override;
  std::unique_ptr<EncoderSelectorInterface> GetEncoderSelector()
      const override {
    return nullptr;
  }

 private:
  std::vector<SdpVideoFormat> supported_formats_;
};

}  // namespace webrtc

#endif  // V4L2_VIDEO_ENCODER_FACTORY_H_
