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

#include <cstdint>
#include <vector>

#include "api/video/video_frame_buffer.h"

namespace livekit {

enum class EncodedVideoCodec {
  kH264,
  kH265,
  kVP8,
  kVP9,
  kAV1,
};

enum class EncodedFrameType {
  kKey,
  kDelta,
};

// A native WebRTC frame buffer carrying one encoded video access unit.
class EncodedVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  EncodedVideoFrameBuffer(int width,
                          int height,
                          EncodedVideoCodec codec,
                          EncodedFrameType frame_type,
                          std::vector<uint8_t> payload);
  ~EncodedVideoFrameBuffer() override = default;

  Type type() const override;
  int width() const override;
  int height() const override;
  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> CropAndScale(
      int offset_x,
      int offset_y,
      int crop_width,
      int crop_height,
      int scaled_width,
      int scaled_height) override;

  EncodedVideoCodec codec() const { return codec_; }
  EncodedFrameType frame_type() const { return frame_type_; }
  const std::vector<uint8_t>& payload() const { return payload_; }

  static EncodedVideoFrameBuffer* FromNative(webrtc::VideoFrameBuffer* buffer);

 private:
  int width_;
  int height_;
  EncodedVideoCodec codec_;
  EncodedFrameType frame_type_;
  std::vector<uint8_t> payload_;
};

}  // namespace livekit
