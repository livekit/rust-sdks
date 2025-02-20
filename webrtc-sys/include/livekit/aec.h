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
#include "modules/audio_processing/aec3/echo_canceller3.h"
#include "modules/audio_processing/audio_buffer.h"

namespace livekit {

struct AecOptions {
  int sample_rate;
  int num_channels;
};

class Aec {
 public:
  Aec(const AecOptions& options);
  ~Aec();

  void cancel_echo(int16_t* cap,
                   size_t cap_len,
                   const int16_t* rend,
                   size_t rend_len);

 private:
  AecOptions options_;
  std::unique_ptr<webrtc::EchoCanceller3> aec3_;
  std::unique_ptr<webrtc::AudioBuffer> cap_buf_;
  std::unique_ptr<webrtc::AudioBuffer> rend_buf_;
};

std::unique_ptr<Aec> create_aec(int sample_rate, int num_channels);

}  // namespace livekit
