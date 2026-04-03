#include "jetson_nvmm_buffer.h"

#include <cstdint>

#include "NvBufSurface.h"
#include "api/video/i420_buffer.h"
#include "third_party/libyuv/include/libyuv/convert.h"

namespace {

bool map_plane_read(NvBufSurface* surface, uint32_t plane) {
  return NvBufSurfaceMap(surface, 0, static_cast<int>(plane), NVBUF_MAP_READ) == 0 &&
         NvBufSurfaceSyncForCpu(surface, 0, static_cast<int>(plane)) == 0;
}

void unmap_plane(NvBufSurface* surface, uint32_t plane) {
  if (surface) {
    NvBufSurfaceUnMap(surface, 0, static_cast<int>(plane));
  }
}

}  // namespace

namespace livekit {

JetsonNvmmBuffer::JetsonNvmmBuffer(
    int dmabuf_fd,
    int width,
    int height,
    int y_stride,
    int uv_stride,
    rust::Box<livekit_ffi::JetsonBufferDropGuard> guard)
    : dmabuf_fd_(dmabuf_fd),
      width_(width),
      height_(height),
      y_stride_(y_stride),
      uv_stride_(uv_stride),
      guard_(std::move(guard)) {}

JetsonNvmmBuffer::~JetsonNvmmBuffer() = default;

webrtc::VideoFrameBuffer::Type JetsonNvmmBuffer::type() const {
  return Type::kNative;
}

int JetsonNvmmBuffer::width() const {
  return width_;
}

int JetsonNvmmBuffer::height() const {
  return height_;
}

rtc::scoped_refptr<webrtc::I420BufferInterface> JetsonNvmmBuffer::ToI420() {
  NvBufSurface* surface = nullptr;
  if (dmabuf_fd_ < 0 ||
      NvBufSurfaceFromFd(dmabuf_fd_, reinterpret_cast<void**>(&surface)) != 0 ||
      !surface || surface->batchSize < 1) {
    return nullptr;
  }

  if (!map_plane_read(surface, 0)) {
    return nullptr;
  }
  if (!map_plane_read(surface, 1)) {
    unmap_plane(surface, 0);
    return nullptr;
  }

  const NvBufSurfaceParams& params = surface->surfaceList[0];
  const uint8_t* src_y =
      static_cast<const uint8_t*>(params.mappedAddr.addr[0]);
  const uint8_t* src_uv =
      static_cast<const uint8_t*>(params.mappedAddr.addr[1]);
  const int src_y_stride =
      params.planeParams.pitch[0] > 0 ? params.planeParams.pitch[0] : y_stride_;
  const int src_uv_stride =
      params.planeParams.pitch[1] > 0 ? params.planeParams.pitch[1] : uv_stride_;

  auto i420 = webrtc::I420Buffer::Create(width_, height_);
  if (!src_y || !src_uv || !i420) {
    unmap_plane(surface, 1);
    unmap_plane(surface, 0);
    return nullptr;
  }

  const int ret = libyuv::NV12ToI420(
      src_y, src_y_stride, src_uv, src_uv_stride, i420->MutableDataY(),
      i420->StrideY(), i420->MutableDataU(), i420->StrideU(),
      i420->MutableDataV(), i420->StrideV(), width_, height_);

  unmap_plane(surface, 1);
  unmap_plane(surface, 0);

  return ret == 0 ? i420 : nullptr;
}

int JetsonNvmmBuffer::dmabuf_fd() const {
  return dmabuf_fd_;
}

int JetsonNvmmBuffer::y_stride() const {
  return y_stride_;
}

int JetsonNvmmBuffer::uv_stride() const {
  return uv_stride_;
}

}  // namespace livekit
