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

#include "livekit/encoded_video_frame_buffer.h"

#include <utility>

#include "rtc_base/logging.h"

namespace livekit {

EncodedVideoFrameBuffer::EncodedVideoFrameBuffer(
    int width,
    int height,
    EncodedVideoCodec codec,
    EncodedFrameType frame_type,
    std::vector<uint8_t> payload)
    : width_(width),
      height_(height),
      codec_(codec),
      frame_type_(frame_type),
      payload_(std::move(payload)) {}

webrtc::VideoFrameBuffer::Type EncodedVideoFrameBuffer::type() const {
  return Type::kNative;
}

int EncodedVideoFrameBuffer::width() const {
  return width_;
}

int EncodedVideoFrameBuffer::height() const {
  return height_;
}

webrtc::scoped_refptr<webrtc::I420BufferInterface>
EncodedVideoFrameBuffer::ToI420() {
  RTC_LOG(LS_ERROR) << "EncodedVideoFrameBuffer::ToI420 is unsupported";
  return nullptr;
}

webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
EncodedVideoFrameBuffer::CropAndScale(int /* offset_x */,
                                      int /* offset_y */,
                                      int /* crop_width */,
                                      int /* crop_height */,
                                      int /* scaled_width */,
                                      int /* scaled_height */) {
  RTC_LOG(LS_ERROR) << "EncodedVideoFrameBuffer::CropAndScale is unsupported";
  return nullptr;
}

EncodedVideoFrameBuffer* EncodedVideoFrameBuffer::FromNative(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != webrtc::VideoFrameBuffer::Type::kNative) {
    return nullptr;
  }
  return dynamic_cast<EncodedVideoFrameBuffer*>(buffer);
}

}  // namespace livekit
