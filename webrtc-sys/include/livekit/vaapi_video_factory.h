/*
 * Copyright 2023 LiveKit
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

 #pragma once

 #include <memory>
 
 #include "api/video_codecs/video_decoder_factory.h"
 #include "api/video_codecs/video_encoder_factory.h"
 
 namespace livekit {
 
 std::unique_ptr<webrtc::VideoEncoderFactory> CreateVaapiVideoEncoderFactory();
 std::unique_ptr<webrtc::VideoDecoderFactory> CreateVaapiVideoDecoderFactory();
 
 } // namespace livekit
 