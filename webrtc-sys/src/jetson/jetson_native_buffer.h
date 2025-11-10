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

#ifndef LIVEKIT_JETSON_NATIVE_BUFFER_H_
#define LIVEKIT_JETSON_NATIVE_BUFFER_H_

#include <memory>

#include "api/video/video_frame_buffer.h"
#include "api/scoped_refptr.h"

namespace livekit {

// A native WebRTC VideoFrameBuffer carrying Jetson DMABUF plane FDs and strides.
// Supports YUV420M (3-plane) and NV12M (2-plane, interleaved UV).
class JetsonDmabufVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  enum class PixelLayout {
    kYUV420M,
    kNV12M,
  };

  webrtc::VideoFrameBuffer::Type type() const override {
    return webrtc::VideoFrameBuffer::Type::kNative;
  }
  int width() const override;
  int height() const override;

  // Slow path not supported here. If conversion is required, upstream should
  // provide CPU buffers.
  rtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  bool is_nv12() const;
  int fd_y() const;
  int fd_u() const;
  int fd_v() const;
  int stride_y() const;
  int stride_u() const;
  int stride_v() const;

 private:
  JetsonDmabufVideoFrameBuffer(int width, int height, PixelLayout layout,
                               int fd_y, int fd_u, int fd_v,
                               int stride_y, int stride_u, int stride_v)
      : width_(width),
        height_(height),
        layout_(layout),
        fd_y_(fd_y),
        fd_u_(fd_u),
        fd_v_(fd_v),
        stride_y_(stride_y),
        stride_u_(stride_u),
        stride_v_(stride_v) {}

  const int width_;
  const int height_;
  const PixelLayout layout_;
  const int fd_y_;
  const int fd_u_;
  const int fd_v_;
  const int stride_y_;
  const int stride_u_;
  const int stride_v_;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_NATIVE_BUFFER_H_


