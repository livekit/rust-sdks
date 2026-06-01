#ifndef LIVEKIT_DMABUF_VIDEO_FRAME_BUFFER_H_
#define LIVEKIT_DMABUF_VIDEO_FRAME_BUFFER_H_

#include <functional>

#include "api/video/i420_buffer.h"
#include "api/video/video_frame_buffer.h"
#include "livekit/video_frame_buffer.h"

namespace livekit_ffi {

class DmaBufVideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  using ReleaseCallback = std::function<void()>;

  DmaBufVideoFrameBuffer(DmaBufVideoFrameDescriptor descriptor,
                         ReleaseCallback release_callback);
  DmaBufVideoFrameBuffer(const DmaBufVideoFrameBuffer&) = delete;
  DmaBufVideoFrameBuffer& operator=(const DmaBufVideoFrameBuffer&) = delete;
  ~DmaBufVideoFrameBuffer() override;

  Type type() const override;
  int width() const override;
  int height() const override;
  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;

  const DmaBufVideoFrameDescriptor& descriptor() const;

 private:
  DmaBufVideoFrameDescriptor descriptor_;
  ReleaseCallback release_callback_;
};

bool GetDmaBufVideoFrameDescriptor(const webrtc::VideoFrameBuffer* buffer,
                                   DmaBufVideoFrameDescriptor* descriptor);

}  // namespace livekit_ffi

#endif  // LIVEKIT_DMABUF_VIDEO_FRAME_BUFFER_H_
