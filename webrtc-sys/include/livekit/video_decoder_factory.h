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
enum class VideoDecoderBackend : std::int32_t;

class VideoDecoderFactory : public webrtc::VideoDecoderFactory {
 public:
  VideoDecoderFactory();
  // With VideoDecoderBackend::PassThrough every decoder is a
  // PassthroughVideoDecoder, and a codec it does not support gets none.
  explicit VideoDecoderFactory(VideoDecoderBackend backend);

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;

  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
                                 bool reference_scaling) const override;

  std::unique_ptr<webrtc::VideoDecoder> Create(
      const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<std::unique_ptr<webrtc::VideoDecoderFactory>> factories_;
  std::unique_ptr<webrtc::VideoDecoderFactory> passthrough_factory_;
  const bool internal_h264_decoder_works_;
};

#ifdef LIVEKIT_TEST
rust::String decoder_factory_create_for_test(int32_t backend,
                                             rust::Str codec,
                                             rust::Str params);
rust::Vec<rust::String> decoder_factory_formats_for_test(int32_t backend);
#endif
}  // namespace livekit_ffi
