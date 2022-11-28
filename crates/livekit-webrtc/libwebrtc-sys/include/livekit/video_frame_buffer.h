//
// Created by theom on 14/11/2022.
//

#ifndef LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H
#define LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H

#include <memory>

#include "api/video/video_frame_buffer.h"
#include "rust_types.h"

namespace livekit {

class PlanarYuvBuffer;
class PlanarYuv8Buffer;
class I420Buffer;

class VideoFrameBuffer {
 public:
  explicit VideoFrameBuffer(rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer)
      : buffer_(std::move(buffer)) {}

  VideoFrameBufferType buffer_type() const {
    return static_cast<VideoFrameBufferType>(buffer_->type());
  }

  int width() const { return buffer_->width(); }
  int height() const { return buffer_->height(); }

  std::unique_ptr<I420Buffer> to_i420() {
    return std::make_unique<I420Buffer>(buffer_->ToI420());
  }

  std::unique_ptr<I420Buffer> get_i420() {
    // const_cast is valid here because we take the ownership on the rust side
    return std::make_unique<I420Buffer>(
        rtc::scoped_refptr<webrtc::I420BufferInterface>(
            const_cast<webrtc::I420BufferInterface*>(buffer_->GetI420())));
  }

 protected:
  rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};

class PlanarYuvBuffer : public VideoFrameBuffer {
 public:
  explicit PlanarYuvBuffer(rtc::scoped_refptr<webrtc::PlanarYuvBuffer> buffer)
      : VideoFrameBuffer(buffer) {}

  int chroma_width() const { return buffer()->ChromaWidth(); }
  int chroma_height() const { return buffer()->ChromaHeight(); }

  int stride_y() const { return buffer()->StrideY(); }
  int stride_u() const { return buffer()->StrideU(); }
  int stride_v() const { return buffer()->StrideV(); }

 private:
  webrtc::PlanarYuvBuffer* buffer() const {
    return static_cast<webrtc::PlanarYuvBuffer*>(buffer_.get());
  }
};

class PlanarYuv8Buffer : public PlanarYuvBuffer {
 public:
  explicit PlanarYuv8Buffer(rtc::scoped_refptr<webrtc::PlanarYuv8Buffer> buffer)
      : PlanarYuvBuffer(buffer) {}

  const uint8_t* data_y() const { return buffer()->DataY(); }
  const uint8_t* data_u() const { return buffer()->DataU(); }
  const uint8_t* data_v() const { return buffer()->DataV(); }

 private:
  webrtc::PlanarYuv8Buffer* buffer() const {
    return static_cast<webrtc::PlanarYuv8Buffer*>(buffer_.get());
  }
};

class I420Buffer : public PlanarYuv8Buffer {
 public:
  explicit I420Buffer(rtc::scoped_refptr<webrtc::I420BufferInterface> buffer)
      : PlanarYuv8Buffer(buffer) {}
};

static const VideoFrameBuffer* yuv_to_vfb(const PlanarYuvBuffer* yuv) {
  return yuv;
}

static const PlanarYuvBuffer* yuv8_to_yuv(const PlanarYuv8Buffer* yuv8) {
  return yuv8;
}

static const PlanarYuv8Buffer* i420_to_yuv8(const I420Buffer* i420) {
  return i420;
}

static std::unique_ptr<VideoFrameBuffer> _unique_video_frame_buffer() {
  return nullptr;  // Ignore
}

}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H
