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
#include <string>
#include <vector>

#include "api/audio/audio_frame.h"
#include "api/audio_codecs/audio_decoder.h"
#include "api/audio_codecs/audio_encoder.h"
#include "api/environment/environment.h"
#include "common_audio/resampler/include/push_resampler.h"
#include "modules/audio_processing/rms_level.h"
#include "rtc_base/buffer.h"
#include "rust/cxx.h"

namespace livekit_ffi {
class NativeOpusEncoder;
class NativeOpusDecoder;
}  // namespace livekit_ffi
#include "webrtc-sys/src/audio_codec.rs.h"

namespace livekit_ffi {

// An Opus encoder created by libwebrtc's builtin audio encoder factory.
// Accepts 10 ms of interleaved PCM at any rate/channel count (remixed and
// resampled to 48 kHz as needed). Not thread-safe; owned by one Rust value.
class NativeOpusEncoder {
 public:
  NativeOpusEncoder();
  std::string Initialize(const OpusEncoderSettings& settings);

  OpusEncodeResult encode(rust::Slice<const int16_t> pcm);
  // The packet the last encode completed (empty when it completed none).
  rust::Slice<const uint8_t> packet() const;
  void set_bitrate(uint32_t bitrate_bps);
  void set_packet_loss_rate(float fraction);
  uint32_t bitrate() const;

 private:
  webrtc::Environment env_;
  std::unique_ptr<webrtc::AudioEncoder> encoder_;
  webrtc::PushResampler<int16_t> resampler_;
  webrtc::AudioFrame resampled_;
  webrtc::RmsLevel rms_level_;
  size_t analyzed_chunks_ = 0;
  webrtc::Buffer encoded_;
  bool has_packet_ = false;
  int input_sample_rate_ = 0;
  size_t input_channels_ = 0;
  uint32_t next_timestamp_ = 0;
};

std::unique_ptr<NativeOpusEncoder> create_opus_encoder(
    const OpusEncoderSettings& settings);

// An Opus decoder created by libwebrtc's builtin audio decoder factory.
// Produces interleaved PCM at the requested rate/channel count. Not
// thread-safe; owned by one Rust value.
class NativeOpusDecoder {
 public:
  NativeOpusDecoder();
  std::string Initialize(uint32_t sample_rate, uint32_t num_channels);

  bool decode(rust::Slice<const uint8_t> packet);
  bool conceal(rust::Slice<const uint8_t> next_packet);
  // The audio the last successful decode or conceal produced.
  rust::Slice<const int16_t> output() const;

 private:
  // Converts `total_samples` interleaved decoder samples to the output
  // format into `output_`.
  bool Output(size_t total_samples);

  webrtc::Environment env_;
  std::unique_ptr<webrtc::AudioDecoder> decoder_;
  std::vector<int16_t> decoded_;
  webrtc::PushResampler<int16_t> resampler_;
  webrtc::AudioFrame resampled_;
  std::vector<int16_t> output_;
  int output_sample_rate_ = 0;
  size_t output_channels_ = 0;
  // Duration of the last decoded packet, per channel at the decoder rate.
  size_t last_packet_samples_ = 960;
};

std::unique_ptr<NativeOpusDecoder> create_opus_decoder(uint32_t sample_rate,
                                                       uint32_t num_channels);

}  // namespace livekit_ffi
