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

#include "api/scoped_refptr.h"
#include "api/video/video_frame_buffer.h"
#include "api/video_codecs/video_decoder.h"

namespace livekit_ffi {

class NullVideoDecoder : public webrtc::VideoDecoder {
 public:
  bool Configure(const Settings& settings) override;
  int32_t Decode(const webrtc::EncodedImage& input_image,
                 int64_t render_time_ms) override;
  int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback* callback) override;
  int32_t Release() override;
  DecoderInfo GetDecoderInfo() const override;

 private:
  webrtc::DecodedImageCallback* callback_ = nullptr;
  webrtc::VideoCodecType codec_type_ = webrtc::kVideoCodecGeneric;
  // Only keyframes carry a size; delta frames reuse the last one seen.
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};

}  // namespace livekit_ffi
