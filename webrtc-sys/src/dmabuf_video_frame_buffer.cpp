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

#include "livekit/dmabuf_video_frame_buffer.h"

#include <cstdio>
#include <cstring>
#include <unordered_map>

#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"

#ifdef USE_JETSON_VIDEO_CODEC
#include "NvBufSurface.h"
#endif

#include "third_party/libyuv/include/libyuv/convert.h"

namespace livekit {

DmaBufVideoFrameBuffer::DmaBufVideoFrameBuffer(int dmabuf_fd,
                                                 int width,
                                                 int height,
                                                 DmaBufPixelFormat pixel_format)
    : dmabuf_fd_(dmabuf_fd),
      width_(width),
      height_(height),
      pixel_format_(pixel_format) {}

webrtc::VideoFrameBuffer::Type DmaBufVideoFrameBuffer::type() const {
  return Type::kNative;
}

int DmaBufVideoFrameBuffer::width() const {
  return width_;
}

int DmaBufVideoFrameBuffer::height() const {
  return height_;
}

rtc::scoped_refptr<webrtc::I420BufferInterface>
DmaBufVideoFrameBuffer::ToI420() {
#ifdef USE_JETSON_VIDEO_CODEC
  // Cache NvBufSurface pointers per fd to avoid calling NvBufSurfaceFromFd
  // on every frame.  On some JetPack versions the fd-to-surface lookup
  // prints spurious "Wrong buffer index" warnings.  The surface pointer is
  // stable for the lifetime of the DMA buffer (freed only when the Argus
  // session is destroyed), so caching is safe.
  static std::unordered_map<int, NvBufSurface*> surface_cache;

  NvBufSurface* surface = nullptr;
  auto cache_it = surface_cache.find(dmabuf_fd_);
  if (cache_it != surface_cache.end()) {
    surface = cache_it->second;
  } else {
    int ret = NvBufSurfaceFromFd(dmabuf_fd_, reinterpret_cast<void**>(&surface));
    if (ret != 0 || !surface || surface->batchSize < 1) {
      RTC_LOG(LS_ERROR) << "DmaBufVideoFrameBuffer::ToI420: "
                           "NvBufSurfaceFromFd failed (fd=" << dmabuf_fd_
                        << ", ret=" << ret << ")";
      return nullptr;
    }
    surface_cache[dmabuf_fd_] = surface;
  }

  int ret = NvBufSurfaceMap(surface, 0, -1, NVBUF_MAP_READ);
  if (ret != 0) {
    RTC_LOG(LS_ERROR) << "DmaBufVideoFrameBuffer::ToI420: "
                         "NvBufSurfaceMap failed (ret=" << ret << ")";
    return nullptr;
  }

  NvBufSurfaceSyncForCpu(surface, 0, -1);

  const NvBufSurfaceParams& params = surface->surfaceList[0];
  rtc::scoped_refptr<webrtc::I420Buffer> i420 =
      webrtc::I420Buffer::Create(width_, height_);

  if (pixel_format_ == DmaBufPixelFormat::kNV12) {
    const uint8_t* src_y =
        static_cast<const uint8_t*>(params.mappedAddr.addr[0]);
    const uint8_t* src_uv =
        static_cast<const uint8_t*>(params.mappedAddr.addr[1]);
    int src_stride_y = static_cast<int>(params.planeParams.pitch[0]);
    int src_stride_uv = static_cast<int>(params.planeParams.pitch[1]);

    libyuv::NV12ToI420(src_y, src_stride_y,
                       src_uv, src_stride_uv,
                       i420->MutableDataY(), i420->StrideY(),
                       i420->MutableDataU(), i420->StrideU(),
                       i420->MutableDataV(), i420->StrideV(),
                       width_, height_);
  } else {
    // YUV420M: three separate planes
    const uint8_t* src_y =
        static_cast<const uint8_t*>(params.mappedAddr.addr[0]);
    const uint8_t* src_u =
        static_cast<const uint8_t*>(params.mappedAddr.addr[1]);
    const uint8_t* src_v =
        static_cast<const uint8_t*>(params.mappedAddr.addr[2]);
    int src_stride_y = static_cast<int>(params.planeParams.pitch[0]);
    int src_stride_u = static_cast<int>(params.planeParams.pitch[1]);
    int src_stride_v = static_cast<int>(params.planeParams.pitch[2]);

    libyuv::I420Copy(src_y, src_stride_y,
                     src_u, src_stride_u,
                     src_v, src_stride_v,
                     i420->MutableDataY(), i420->StrideY(),
                     i420->MutableDataU(), i420->StrideU(),
                     i420->MutableDataV(), i420->StrideV(),
                     width_, height_);
  }

  NvBufSurfaceUnMap(surface, 0, -1);
  return i420;
#else
  RTC_LOG(LS_ERROR) << "DmaBufVideoFrameBuffer::ToI420: "
                       "not supported without Jetson MMAPI";
  return nullptr;
#endif
}

DmaBufVideoFrameBuffer* DmaBufVideoFrameBuffer::FromNative(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != webrtc::VideoFrameBuffer::Type::kNative) {
    return nullptr;
  }
  return dynamic_cast<DmaBufVideoFrameBuffer*>(buffer);
}

}  // namespace livekit
