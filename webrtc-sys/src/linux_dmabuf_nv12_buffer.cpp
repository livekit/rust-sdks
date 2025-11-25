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

#if defined(__linux__)

#include "livekit/linux_dmabuf_nv12_buffer.h"

#include <algorithm>
#include <cerrno>
#include <cstring>

#include "libyuv/convert.h"

namespace livekit {

bool LinuxDmaBufNV12Buffer::MapOnce(uint8_t** y,
                                    uint8_t** uv,
                                    size_t* y_size,
                                    size_t* uv_size) const {
  if (mapped_) {
    uint8_t* base = static_cast<uint8_t*>(mapped_);
    *y = base + offset_y_;
    *uv = base + offset_uv_;
    *y_size = static_cast<size_t>(stride_y_) * static_cast<size_t>(height_);
    *uv_size = static_cast<size_t>(stride_uv_) * ((static_cast<size_t>(height_) + 1) / 2);
    return true;
  }

  // Compute a conservative length to map both planes.
  size_t end_y = static_cast<size_t>(offset_y_) +
                 static_cast<size_t>(stride_y_) * static_cast<size_t>(height_);
  size_t end_uv = static_cast<size_t>(offset_uv_) +
                  static_cast<size_t>(stride_uv_) * ((static_cast<size_t>(height_) + 1) / 2);
  size_t map_len = std::max(end_y, end_uv);

  void* ptr = ::mmap(nullptr, map_len, PROT_READ, MAP_SHARED, fd_, 0);
  if (ptr == MAP_FAILED) {
    return false;
  }
  mapped_ = ptr;
  mapped_len_ = map_len;
  uint8_t* base = static_cast<uint8_t*>(mapped_);
  *y = base + offset_y_;
  *uv = base + offset_uv_;
  *y_size = static_cast<size_t>(stride_y_) * static_cast<size_t>(height_);
  *uv_size = static_cast<size_t>(stride_uv_) * ((static_cast<size_t>(height_) + 1) / 2);
  return true;
}

rtc::scoped_refptr<webrtc::I420BufferInterface> LinuxDmaBufNV12Buffer::ToI420() {
  uint8_t* y = nullptr;
  uint8_t* uv = nullptr;
  size_t y_size = 0;
  size_t uv_size = 0;
  if (!MapOnce(&y, &uv, &y_size, &uv_size)) {
    // Fallback to empty black frame if we cannot map.
    return webrtc::I420Buffer::Create(width_, height_);
  }

  auto i420 = webrtc::I420Buffer::Create(width_, height_);
  int chroma_h = (height_ + 1) / 2;
  int res = libyuv::NV12ToI420(
      y, stride_y_,
      uv, stride_uv_,
      i420->MutableDataY(), i420->StrideY(),
      i420->MutableDataU(), i420->StrideU(),
      i420->MutableDataV(), i420->StrideV(),
      width_, height_);
  if (res != 0) {
    // Return blank buffer on failure; avoids crashing the pipeline.
    return webrtc::I420Buffer::Create(width_, height_);
  }
  return i420;
}

}  // namespace livekit

#endif  // __linux__


