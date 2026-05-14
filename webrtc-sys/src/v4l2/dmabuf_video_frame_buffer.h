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

#ifndef DMABUF_VIDEO_FRAME_BUFFER_H_
#define DMABUF_VIDEO_FRAME_BUFFER_H_

#include <array>
#include <cstddef>
#include <cstdint>

#include "api/scoped_refptr.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame_buffer.h"

namespace livekit_ffi {

// A native-handle `webrtc::VideoFrameBuffer` that wraps a Linux DMABUF
// file descriptor. Up to three planes are supported (Y/U/V for YUV420
// or Y/UV for NV12).
//
// The buffer is created via `Wrap()`; the constructor duplicates the
// caller's file descriptor so the caller may safely close its handle
// after construction. The duplicated fd is closed when the last
// `scoped_refptr` is dropped.
//
// `ToI420()` performs an mmap'd CPU fallback conversion via libyuv so
// non-DMABUF-aware encoders (and any consumer that asks for I420) still
// work transparently.
class DmabufVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  static constexpr size_t kMaxPlanes = 3;

  struct Plane {
    size_t offset = 0;
    int stride = 0;
  };

  // Construct a refcounted DMABUF buffer. The fd is dup()'d; the caller
  // retains ownership of the original handle.
  //
  // `total_size` is the byte length of the entire mapped region (sum of
  // plane offsets + last plane size).
  //
  // `colorspace_v4l2` carries a V4L2-style `v4l2_colorspace` value
  // (e.g. `V4L2_COLORSPACE_REC709 == 3`,
  // `V4L2_COLORSPACE_SMPTE170M == 1`). Use 0 (`V4L2_COLORSPACE_DEFAULT`)
  // to let the encoder pick its built-in default. The V4L2 H.264 encoder
  // wrapper passes this value into `S_FMT` on the OUTPUT queue so the
  // hardware encoder is told the producer's actual colorspace.
  //
  // Returns null on failure (invalid fd, dup() failure, or unsupported
  // fourcc).
  static webrtc::scoped_refptr<DmabufVideoFrameBuffer> Wrap(
      int dmabuf_fd,
      uint32_t fourcc,
      int width,
      int height,
      size_t total_size,
      const Plane* planes,
      size_t num_planes,
      uint32_t colorspace_v4l2 = 0);

  // Down-cast helper: returns non-null only when `buffer` is a
  // DmabufVideoFrameBuffer.
  static DmabufVideoFrameBuffer* TryCast(webrtc::VideoFrameBuffer* buffer);

  // --- webrtc::VideoFrameBuffer ---
  Type type() const override { return Type::kNative; }
  int width() const override { return width_; }
  int height() const override { return height_; }

  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  // --- DMABUF accessors ---

  // The duplicated dmabuf fd owned by this buffer. Lives for as long as
  // the buffer is referenced.
  int dmabuf_fd() const { return dmabuf_fd_; }

  // V4L2-style fourcc (e.g. V4L2_PIX_FMT_YUV420 == 'YU12').
  uint32_t fourcc() const { return fourcc_; }

  size_t num_planes() const { return num_planes_; }
  size_t plane_offset(size_t idx) const { return planes_[idx].offset; }
  int plane_stride(size_t idx) const { return planes_[idx].stride; }

  // Total byte length of the dmabuf region (sum of all plane data).
  // Used by the V4L2 encoder as the OUTPUT plane bytesused/length.
  size_t total_size() const { return total_size_; }

  // V4L2-style colorspace (e.g. V4L2_COLORSPACE_REC709 == 3). 0 means
  // "unspecified, use encoder default". See the comment on `Wrap()`
  // above for how this is consumed by the V4L2 H.264 encoder.
  uint32_t colorspace_v4l2() const { return colorspace_v4l2_; }

 protected:
  DmabufVideoFrameBuffer(int dmabuf_fd,
                         uint32_t fourcc,
                         int width,
                         int height,
                         size_t total_size,
                         const Plane* planes,
                         size_t num_planes,
                         uint32_t colorspace_v4l2);
  ~DmabufVideoFrameBuffer() override;

 private:
  int dmabuf_fd_ = -1;
  uint32_t fourcc_ = 0;
  int width_ = 0;
  int height_ = 0;
  size_t total_size_ = 0;
  std::array<Plane, kMaxPlanes> planes_{};
  size_t num_planes_ = 0;
  uint32_t colorspace_v4l2_ = 0;
};

}  // namespace livekit_ffi

#endif  // DMABUF_VIDEO_FRAME_BUFFER_H_
