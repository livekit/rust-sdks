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

#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "absl/strings/match.h"
#ifdef LIVEKIT_TEST
#include "rust/cxx.h"
#endif

namespace livekit_ffi {
class VideoDecoderFactory : public webrtc::VideoDecoderFactory {
 public:
  VideoDecoderFactory();

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
                                 bool reference_scaling) const override;

  std::unique_ptr<webrtc::VideoDecoder> Create(
      const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoDecoderFactory>> factories_;
  const bool internal_h264_decoder_works_;
};

#ifdef LIVEKIT_TEST
// Status, the first value returned by jetson_h265_decode_for_test().
constexpr uint32_t kJetsonDecodeUnsupported = 0;
constexpr uint32_t kJetsonDecodeOk = 1;
constexpr uint32_t kJetsonDecodeConfigureFailed = 2;

// Decodes `access_units` (each prefixed with its little-endian u32 size) with
// the Jetson hardware H.265 decoder, giving access unit i the RTP timestamp
// (i + 1) * 3000. The access unit at `corrupt_index` has its payload
// scrambled first. With `release_immediately` the decoder is released right
// after the last access unit is queued, instead of after its pictures arrive.
//
// Returns [status, accepted, rejected] followed by [width, height,
// rtp_timestamp] for every decoded picture, in delivery order.
rust::Vec<uint32_t> jetson_h265_decode_for_test(
    rust::Slice<const uint8_t> access_units,
    uint32_t corrupt_index,
    bool release_immediately);
#endif
}  // namespace livekit_ffi
