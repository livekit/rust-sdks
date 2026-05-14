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

#include "dmabuf_video_frame_buffer.h"

#include <fcntl.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

#include <linux/dma-buf.h>

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"
#include "third_party/libyuv/include/libyuv/convert.h"
#include "third_party/libyuv/include/libyuv/planar_functions.h"

namespace livekit_ffi {

namespace {

// V4L2 fourccs we natively understand. Defined inline to avoid pulling
// in <linux/videodev2.h> from this header.
constexpr uint32_t kFourccYUV420 = 0x32315559;  // 'YU12'
constexpr uint32_t kFourccNV12 = 0x3231564E;    // 'NV12'
constexpr int kH264MacroblockAlignment = 16;

int AlignUp(int value, int alignment) {
  return ((value + alignment - 1) / alignment) * alignment;
}

int ChromaHeight(int height) {
  return (height + 1) / 2;
}

size_t Yuv420Size(int stride_y, int storage_luma_height) {
  const int stride_uv = stride_y / 2;
  return static_cast<size_t>(stride_y) * storage_luma_height +
         2u * static_cast<size_t>(stride_uv) *
             ChromaHeight(storage_luma_height);
}

size_t Nv12Size(int stride_y, int storage_luma_height) {
  return static_cast<size_t>(stride_y) * storage_luma_height +
         static_cast<size_t>(stride_y) * ChromaHeight(storage_luma_height);
}

int InferSinglePlaneStorageHeight(int visible_height,
                                  int stride_y,
                                  size_t available,
                                  bool yuv420) {
  const int aligned_height =
      AlignUp(visible_height, kH264MacroblockAlignment);
  const size_t aligned_size =
      yuv420 ? Yuv420Size(stride_y, aligned_height)
             : Nv12Size(stride_y, aligned_height);
  if (available >= aligned_size) {
    return aligned_height;
  }

  const size_t visible_size =
      yuv420 ? Yuv420Size(stride_y, visible_height)
             : Nv12Size(stride_y, visible_height);
  return available >= visible_size ? visible_height : 0;
}

// Best-effort DMA-BUF CPU access synchronization. Some DMA-BUF exporters
// (e.g. CMA) don't require this; some (e.g. udmabuf) do. Failures are
// non-fatal but logged once.
void DmabufSync(int fd, bool start, bool read_only) {
  struct dma_buf_sync sync = {};
  sync.flags = (start ? DMA_BUF_SYNC_START : DMA_BUF_SYNC_END) |
               (read_only ? DMA_BUF_SYNC_READ : DMA_BUF_SYNC_RW);
  if (ioctl(fd, DMA_BUF_IOCTL_SYNC, &sync) < 0) {
    static bool warned = false;
    if (!warned) {
      RTC_LOG(LS_WARNING) << "DMABUF: DMA_BUF_IOCTL_SYNC failed: "
                          << strerror(errno) << " (further warnings suppressed)";
      warned = true;
    }
  }
}

}  // namespace

// ---------------------------------------------------------------------------
// Construction / lifetime
// ---------------------------------------------------------------------------

DmabufVideoFrameBuffer::DmabufVideoFrameBuffer(int dmabuf_fd,
                                                uint32_t fourcc,
                                                int width,
                                                int height,
                                                size_t total_size,
                                                const Plane* planes,
                                                size_t num_planes,
                                                uint32_t colorspace_v4l2)
    : dmabuf_fd_(dmabuf_fd),
      fourcc_(fourcc),
      width_(width),
      height_(height),
      total_size_(total_size),
      num_planes_(num_planes),
      colorspace_v4l2_(colorspace_v4l2) {
  for (size_t i = 0; i < num_planes; ++i) {
    planes_[i] = planes[i];
  }
}

DmabufVideoFrameBuffer::~DmabufVideoFrameBuffer() {
  if (dmabuf_fd_ >= 0) {
    close(dmabuf_fd_);
    dmabuf_fd_ = -1;
  }
}

webrtc::scoped_refptr<DmabufVideoFrameBuffer> DmabufVideoFrameBuffer::Wrap(
    int dmabuf_fd,
    uint32_t fourcc,
    int width,
    int height,
    size_t total_size,
    const Plane* planes,
    size_t num_planes,
    uint32_t colorspace_v4l2) {
  if (dmabuf_fd < 0 || width <= 0 || height <= 0 || num_planes == 0 ||
      num_planes > kMaxPlanes || total_size == 0) {
    return nullptr;
  }
  if (fourcc != kFourccYUV420 && fourcc != kFourccNV12) {
    // webrtc's RTC_LOG streamer doesn't support iostream manipulators like
    // std::hex, so format the fourcc into a stack buffer first.
    char fourcc_str[16];
    std::snprintf(fourcc_str, sizeof(fourcc_str), "0x%08x", fourcc);
    RTC_LOG(LS_WARNING) << "DMABUF: unsupported fourcc " << fourcc_str
                        << "; only YUV420 and NV12 are supported";
    return nullptr;
  }
  for (size_t i = 0; i < num_planes; ++i) {
    if (planes[i].stride <= 0 || planes[i].offset >= total_size) {
      RTC_LOG(LS_WARNING) << "DMABUF: invalid plane " << i
                          << " offset=" << planes[i].offset
                          << " stride=" << planes[i].stride
                          << " total_size=" << total_size;
      return nullptr;
    }
  }

  // Dup the fd so the buffer owns its own copy. The caller can safely
  // close the original after this returns.
  int dup_fd = fcntl(dmabuf_fd, F_DUPFD_CLOEXEC, 0);
  if (dup_fd < 0) {
    RTC_LOG(LS_ERROR) << "DMABUF: fcntl(F_DUPFD_CLOEXEC) failed: "
                      << strerror(errno);
    return nullptr;
  }

  return webrtc::make_ref_counted<DmabufVideoFrameBuffer>(
      dup_fd, fourcc, width, height, total_size, planes, num_planes,
      colorspace_v4l2);
}

DmabufVideoFrameBuffer* DmabufVideoFrameBuffer::TryCast(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != Type::kNative) {
    return nullptr;
  }
  // RTTI is typically disabled in WebRTC builds, so we can't use
  // dynamic_cast. The cast is sound here because this is the only kNative
  // VideoFrameBuffer kind the Linux SDK ever produces (NativeBuffer::
  // from_dmabuf) and the V4L2 encoder is the only consumer on Linux.
  // The Apple CVPixelBuffer-backed kNative buffer lives on a different
  // platform and never reaches this code path.
  return static_cast<DmabufVideoFrameBuffer*>(buffer);
}

// ---------------------------------------------------------------------------
// ToI420 fallback (mmap + libyuv)
// ---------------------------------------------------------------------------

webrtc::scoped_refptr<webrtc::I420BufferInterface>
DmabufVideoFrameBuffer::ToI420() {
  // Map the dmabuf into our address space for CPU access. Mapping the
  // entire region (rather than per-plane) is simpler and matches the
  // single-fd export pattern that libcamera uses.
  void* mapped =
      mmap(nullptr, total_size_, PROT_READ, MAP_SHARED, dmabuf_fd_, 0);
  if (mapped == MAP_FAILED) {
    RTC_LOG(LS_ERROR) << "DMABUF: mmap(" << total_size_
                      << ") failed: " << strerror(errno);
    return nullptr;
  }

  DmabufSync(dmabuf_fd_, /*start=*/true, /*read_only=*/true);

  webrtc::scoped_refptr<webrtc::I420Buffer> dst =
      webrtc::I420Buffer::Create(width_, height_);
  if (!dst) {
    DmabufSync(dmabuf_fd_, /*start=*/false, /*read_only=*/true);
    munmap(mapped, total_size_);
    return nullptr;
  }

  const uint8_t* base = static_cast<const uint8_t*>(mapped);

  if (fourcc_ == kFourccYUV420) {
    const int storage_luma_height = num_planes_ == 1
                                        ? InferSinglePlaneStorageHeight(
                                              height_, planes_[0].stride,
                                              total_size_ - planes_[0].offset,
                                              /*yuv420=*/true)
                                        : height_;
    if (storage_luma_height == 0) {
      RTC_LOG(LS_ERROR) << "DMABUF: YUV420 buffer is too small for "
                        << width_ << "x" << height_;
      DmabufSync(dmabuf_fd_, /*start=*/false, /*read_only=*/true);
      munmap(mapped, total_size_);
      return nullptr;
    }
    const int storage_chroma_height = ChromaHeight(storage_luma_height);
    const uint8_t* y = base + planes_[0].offset;
    const uint8_t* u = num_planes_ > 1 ? base + planes_[1].offset
                                       : y + planes_[0].stride *
                                                 storage_luma_height;
    const uint8_t* v = num_planes_ > 2
                           ? base + planes_[2].offset
                           : u + (num_planes_ > 1 ? planes_[1].stride : planes_[0].stride / 2) *
                                     storage_chroma_height;
    const int src_stride_y = planes_[0].stride;
    const int src_stride_u =
        num_planes_ > 1 ? planes_[1].stride : src_stride_y / 2;
    const int src_stride_v =
        num_planes_ > 2 ? planes_[2].stride : src_stride_u;
    libyuv::I420Copy(y, src_stride_y, u, src_stride_u, v, src_stride_v,
                     dst->MutableDataY(), dst->StrideY(),
                     dst->MutableDataU(), dst->StrideU(),
                     dst->MutableDataV(), dst->StrideV(), width_, height_);
  } else {
    // NV12: Y plane followed by interleaved UV.
    const int storage_luma_height = num_planes_ == 1
                                        ? InferSinglePlaneStorageHeight(
                                              height_, planes_[0].stride,
                                              total_size_ - planes_[0].offset,
                                              /*yuv420=*/false)
                                        : height_;
    if (storage_luma_height == 0) {
      RTC_LOG(LS_ERROR) << "DMABUF: NV12 buffer is too small for "
                        << width_ << "x" << height_;
      DmabufSync(dmabuf_fd_, /*start=*/false, /*read_only=*/true);
      munmap(mapped, total_size_);
      return nullptr;
    }
    const uint8_t* y = base + planes_[0].offset;
    const uint8_t* uv = num_planes_ > 1 ? base + planes_[1].offset
                                        : y + planes_[0].stride *
                                                  storage_luma_height;
    const int src_stride_y = planes_[0].stride;
    const int src_stride_uv =
        num_planes_ > 1 ? planes_[1].stride : src_stride_y;
    libyuv::NV12ToI420(y, src_stride_y, uv, src_stride_uv,
                        dst->MutableDataY(), dst->StrideY(),
                        dst->MutableDataU(), dst->StrideU(),
                        dst->MutableDataV(), dst->StrideV(), width_, height_);
  }

  DmabufSync(dmabuf_fd_, /*start=*/false, /*read_only=*/true);
  munmap(mapped, total_size_);

  return dst;
}

}  // namespace livekit_ffi
