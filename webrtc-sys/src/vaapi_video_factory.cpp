/*
 * Copyright 2025 LiveKit
 *
 * Licensed under the Apache License, Version 2.0 (the “License”);
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an “AS IS” BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "livekit/vaapi_video_factory.h"

#include "vaapi/vaapi_encoder.h"
#include "vaapi/vaapi_encoder_factory.h"

namespace livekit {
std::unique_ptr<webrtc::VideoEncoderFactory> CreateVaapiVideoEncoderFactory() {
  return std::make_unique<webrtc::VAAPIVideoEncoderFactory>();
}
std::unique_ptr<webrtc::VideoDecoderFactory> CreateVaapiVideoDecoderFactory() {
  // Implementation of the decoder factory creation
  // This is a placeholder, actual implementation will depend on the specific
  // requirements
  return nullptr;
}
}  // namespace livekit