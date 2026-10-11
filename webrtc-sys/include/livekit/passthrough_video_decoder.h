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

#include <atomic>
#include <cstdint>
#include <memory>
#include <vector>

#include "api/environment/environment.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_decoder.h"
#include "api/video_codecs/video_decoder_factory.h"
#include "livekit/encoded_video_frame_buffer.h"

#ifdef LIVEKIT_TEST
#include "rust/cxx.h"
#endif

namespace livekit_ffi {

// A webrtc::VideoDecoder that does not decode. Each received access unit is
// handed to the sink unchanged, inside an EncodedVideoFrameBuffer, so the
// application can decode it itself (the receive-side mirror of
// PassthroughVideoEncoder).
//
// The buffers it emits share one keyframe-request flag with the decoder:
// calling request_keyframe() on any of them makes the next Decode() return
// WEBRTC_VIDEO_CODEC_OK_REQUEST_KEYFRAME, which asks the sender for a keyframe.
class PassthroughVideoDecoder final : public webrtc::VideoDecoder {
 public:
  explicit PassthroughVideoDecoder(livekit::EncodedVideoCodec codec);
  ~PassthroughVideoDecoder() override = default;

  bool Configure(const Settings& settings) override;
  int32_t Decode(const webrtc::EncodedImage& input_image,
                 int64_t render_time_ms) override;
  int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback* callback) override;
  int32_t Release() override;
  DecoderInfo GetDecoderInfo() const override;
  const char* ImplementationName() const override;

  // Shared with every buffer this decoder emits.
  std::shared_ptr<std::atomic<bool>> keyframe_request_flag() const {
    return keyframe_request_flag_;
  }

 private:
  const livekit::EncodedVideoCodec codec_;
  const void* const stream_id_;
  webrtc::DecodedImageCallback* callback_ = nullptr;
  int width_ = 0;
  int height_ = 0;
  uint64_t sequence_number_ = 0;
  std::shared_ptr<std::atomic<bool>> keyframe_request_flag_ =
      std::make_shared<std::atomic<bool>>(false);
};

// Creates PassthroughVideoDecoders. Codecs are matched by name only: SDP
// profile parameters do not constrain a decoder that does not decode.
class PassthroughVideoDecoderFactory final
    : public webrtc::VideoDecoderFactory {
 public:
  PassthroughVideoDecoderFactory();
  ~PassthroughVideoDecoderFactory() override = default;

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override;
  CodecSupport QueryCodecSupport(const webrtc::SdpVideoFormat& format,
                                 bool reference_scaling) const override;
  std::unique_ptr<webrtc::VideoDecoder> Create(
      const webrtc::Environment& env,
      const webrtc::SdpVideoFormat& format) override;

 private:
  std::vector<webrtc::SdpVideoFormat> supported_formats_;
};

#ifdef LIVEKIT_TEST
// Op codes for passthrough_decode_for_test().
constexpr uint8_t kDecodeTestKey64x64 = 0;
constexpr uint8_t kDecodeTestKey128x72 = 1;
constexpr uint8_t kDecodeTestKeyNoDims = 2;
constexpr uint8_t kDecodeTestDelta = 3;
constexpr uint8_t kDecodeTestSetKeyframeFlag = 4;

struct DecodeForTest;

rust::Vec<DecodeForTest> passthrough_decode_for_test(
    rust::Str codec,
    rust::Slice<const uint8_t> script,
    rust::Slice<const uint8_t> payload);
rust::String passthrough_decoder_info_for_test();
rust::Vec<size_t> passthrough_decoder_identities_for_test();
#endif

}  // namespace livekit_ffi
