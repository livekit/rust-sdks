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

#include "api/video/video_frame_buffer.h"
#include "api/video/i420_buffer.h"

namespace livekit {

// Pixel format of the DMA buffer surface.
enum class DmaBufPixelFormat {
  kNV12 = 0,
  kYUV420M = 1,
};

// A VideoFrameBuffer backed by a Jetson NvBufSurface DMA file descriptor.
// Reports Type::kNative so it flows through the standard WebRTC pipeline.
// The encoder can detect this type and pass the fd directly to the hardware
// encoder via V4L2_MEMORY_DMABUF for zero-copy encoding.
class DmaBufVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  DmaBufVideoFrameBuffer(int dmabuf_fd,
                         int width,
                         int height,
                         DmaBufPixelFormat pixel_format);
  ~DmaBufVideoFrameBuffer() override = default;

  // webrtc::VideoFrameBuffer
  Type type() const override;
  int width() const override;
  int height() const override;
  rtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  // DMA buffer accessors
  int dmabuf_fd() const { return dmabuf_fd_; }
  DmaBufPixelFormat pixel_format() const { return pixel_format_; }

  // Helper to check if a VideoFrameBuffer is a DmaBufVideoFrameBuffer.
  static DmaBufVideoFrameBuffer* FromNative(webrtc::VideoFrameBuffer* buffer);

 private:
  int dmabuf_fd_;
  int width_;
  int height_;
  DmaBufPixelFormat pixel_format_;
};

}  // namespace livekit
