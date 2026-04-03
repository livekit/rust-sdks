#include "jetson_nvmm_buffer.h"

#include <cstdio>
#include <execinfo.h>

#include "rtc_base/logging.h"

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
  RTC_LOG(LS_ERROR)
      << "JetsonNvmmBuffer::ToI420() was called unexpectedly. "
         "Zero-copy Jetson NVMM buffers must not fall back to I420.";
  std::fprintf(stderr,
               "[JetsonNvmmBuffer] ToI420() called unexpectedly for fd=%d "
               "(%dx%d, y_stride=%d, uv_stride=%d)\n",
               dmabuf_fd_, width_, height_, y_stride_, uv_stride_);
  void* frames[32];
  const int frame_count = backtrace(frames, 32);
  if (frame_count > 0) {
    std::fprintf(stderr, "[JetsonNvmmBuffer] Backtrace (%d frames):\n",
                 frame_count);
    backtrace_symbols_fd(frames, frame_count, fileno(stderr));
  }
  std::fflush(stderr);
  return nullptr;
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
