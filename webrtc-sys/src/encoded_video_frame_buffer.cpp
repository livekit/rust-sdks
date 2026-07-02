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

#include "api/video/i420_buffer.h"
#include "rtc_base/logging.h"

namespace livekit {

EncodedVideoFrameBuffer::EncodedVideoFrameBuffer(
    int width,
    int height,
    EncodedVideoCodec codec,
    EncodedFrameType frame_type,
    webrtc::scoped_refptr<webrtc::EncodedImageBuffer> payload,
    std::shared_ptr<std::atomic<bool>> keyframe_request_flag)
    : width_(width),
      height_(height),
      codec_(codec),
      frame_type_(frame_type),
      payload_(std::move(payload)),
      keyframe_request_flag_(std::move(keyframe_request_flag)) {}

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
  // Sinks attached to a pre-encoded track (local preview, FFI color
  // conversion) convert whatever buffer they receive; the encoded payload
  // cannot be decoded here, so hand back a black frame instead of a null
  // buffer that would crash the caller.
  static std::atomic<bool> logged{false};
  if (!logged.exchange(true)) {
    RTC_LOG(LS_WARNING) << "EncodedVideoFrameBuffer::ToI420 cannot decode an "
                           "encoded access unit; returning black frames";
  }
  webrtc::scoped_refptr<webrtc::I420Buffer> buffer =
      webrtc::I420Buffer::Create(width_, height_);
  webrtc::I420Buffer::SetBlack(buffer.get());
  return buffer;
}

webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
EncodedVideoFrameBuffer::CropAndScale(int /* offset_x */,
                                      int /* offset_y */,
                                      int /* crop_width */,
                                      int /* crop_height */,
                                      int /* scaled_width */,
                                      int /* scaled_height */) {
  // Encoded payloads cannot be rescaled; returning the buffer unchanged
  // keeps misbehaving callers alive (the capture path never scales encoded
  // frames).
  RTC_LOG(LS_WARNING) << "EncodedVideoFrameBuffer::CropAndScale is "
                         "unsupported; returning the frame unscaled";
  return webrtc::scoped_refptr<webrtc::VideoFrameBuffer>(this);
}

void EncodedVideoFrameBuffer::request_keyframe() const {
  if (keyframe_request_flag_) {
    keyframe_request_flag_->store(true, std::memory_order_relaxed);
  }
}

EncodedVideoFrameBuffer* EncodedVideoFrameBuffer::FromNative(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != webrtc::VideoFrameBuffer::Type::kNative) {
    return nullptr;
  }
  return dynamic_cast<EncodedVideoFrameBuffer*>(buffer);
}

}  // namespace livekit
