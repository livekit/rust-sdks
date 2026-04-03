#ifndef LIVEKIT_JETSON_NVMM_BUFFER_H_
#define LIVEKIT_JETSON_NVMM_BUFFER_H_

#include <cstdint>

#include "api/video/i420_buffer.h"
#include "api/video/video_frame_buffer.h"
#include "livekit/video_frame_buffer.h"
#include "rust/cxx.h"

namespace livekit {

class JetsonNvmmBuffer : public webrtc::VideoFrameBuffer {
 public:
  JetsonNvmmBuffer(int dmabuf_fd,
                   int width,
                   int height,
                   int y_stride,
                   int uv_stride,
                   rust::Box<livekit_ffi::JetsonBufferDropGuard> guard);
  ~JetsonNvmmBuffer() override;

  Type type() const override;
  int width() const override;
  int height() const override;
  rtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  int dmabuf_fd() const;
  int y_stride() const;
  int uv_stride() const;

 private:
  int dmabuf_fd_;
  int width_;
  int height_;
  int y_stride_;
  int uv_stride_;
  rust::Box<livekit_ffi::JetsonBufferDropGuard> guard_;
};

}  // namespace livekit

#endif  // LIVEKIT_JETSON_NVMM_BUFFER_H_
