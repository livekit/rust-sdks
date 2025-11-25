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

#if defined(__linux__)

#include <memory>

#include <unistd.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <sys/stat.h>
#include <fcntl.h>

#include "api/scoped_refptr.h"
#include "api/video/video_frame_buffer.h"
#include "api/video/i420_buffer.h"
#include "rtc_base/ref_counted_object.h"

namespace livekit {

// Minimal dmabuf-backed NV12 wrapper for WebRTC. Exposes kNative type so higher
// layers can detect and pass directly to platform encoders. ToI420() maps the
// dmabuf and converts to I420 as a fallback.
class LinuxDmaBufNV12Buffer : public webrtc::VideoFrameBuffer {
 public:
  static rtc::scoped_refptr<LinuxDmaBufNV12Buffer> Create(
      int fd,
      int width,
      int height,
      int stride_y,
      int stride_uv,
      int offset_y,
      int offset_uv) {
    return rtc::make_ref_counted<LinuxDmaBufNV12Buffer>(
        fd, width, height, stride_y, stride_uv, offset_y, offset_uv);
  }

  LinuxDmaBufNV12Buffer(int fd,
                        int width,
                        int height,
                        int stride_y,
                        int stride_uv,
                        int offset_y,
                        int offset_uv)
      : fd_(::dup(fd)),
        width_(width),
        height_(height),
        stride_y_(stride_y),
        stride_uv_(stride_uv),
        offset_y_(offset_y),
        offset_uv_(offset_uv),
        mapped_(nullptr),
        mapped_len_(0) {}

  ~LinuxDmaBufNV12Buffer() override {
    if (mapped_) {
      ::munmap(mapped_, mapped_len_);
      mapped_ = nullptr;
      mapped_len_ = 0;
    }
    if (fd_ >= 0) {
      ::close(fd_);
      fd_ = -1;
    }
  }

  Type type() const override { return Type::kNative; }

  int width() const override { return width_; }
  int height() const override { return height_; }

  // Fallback conversion path: map and convert to I420.
  rtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  // Accessors for encoders that can consume dmabuf directly
  int dmabuf_fd() const { return fd_; }
  int stride_y() const { return stride_y_; }
  int stride_uv() const { return stride_uv_; }
  int offset_y() const { return offset_y_; }
  int offset_uv() const { return offset_uv_; }

 private:
  bool MapOnce(uint8_t** y, uint8_t** uv, size_t* y_size, size_t* uv_size) const;

  mutable int fd_;
  int width_;
  int height_;
  int stride_y_;
  int stride_uv_;
  int offset_y_;
  int offset_uv_;

  mutable void* mapped_;
  mutable size_t mapped_len_;
};

}  // namespace livekit

#endif  // __linux__


