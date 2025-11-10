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

#ifndef LIVEKIT_JETSON_VIDEO_ENCODER_FACTORY_H_
#define LIVEKIT_JETSON_VIDEO_ENCODER_FACTORY_H_

#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder_factory.h"

namespace webrtc {

// Factory for Jetson H264 encoder (V4L2 nvv4l2h264enc backend).
class JetsonVideoEncoderFactory : public VideoEncoderFactory {
 public:
  JetsonVideoEncoderFactory();
  ~JetsonVideoEncoderFactory() override;

  // Runtime detection of Jetson encoder availability.
  // This checks for common Jetson-specific components like the V4L2 NVENC
  // device and the GStreamer NV V4L2 plugin. Returns true if available.
  static bool IsSupported();

  std::unique_ptr<VideoEncoder> Create(const Environment& env,
                                       const SdpVideoFormat& format) override;

  // Returns a list of supported codecs in order of preference.
  std::vector<SdpVideoFormat> GetSupportedFormats() const override;

  std::vector<SdpVideoFormat> GetImplementations() const override;

  std::unique_ptr<EncoderSelectorInterface> GetEncoderSelector() const override {
    return nullptr;
  }

 private:
  std::vector<SdpVideoFormat> supported_formats_;
};

}  // namespace webrtc

#endif  // LIVEKIT_JETSON_VIDEO_ENCODER_FACTORY_H_


