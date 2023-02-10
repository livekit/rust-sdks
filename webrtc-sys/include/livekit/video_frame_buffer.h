//
// Created by theom on 14/11/2022.
//

#ifndef LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H
#define LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H

#include <memory>

#include "api/video/video_frame_buffer.h"
#include "rust_types.h"

namespace livekit {

class I420Buffer;
class I420ABuffer;
class I422Buffer;
class I444Buffer;
class I010Buffer;
class NV12Buffer;

class VideoFrameBuffer {
 public:
  explicit VideoFrameBuffer(
      rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer);

  VideoFrameBufferType buffer_type() const;

  int width() const;
  int height() const;

  std::unique_ptr<I420Buffer> to_i420();
  std::unique_ptr<I420Buffer> get_i420();
  std::unique_ptr<I420ABuffer> get_i420a();
  std::unique_ptr<I422Buffer> get_i422();
  std::unique_ptr<I444Buffer> get_i444();
  std::unique_ptr<I010Buffer> get_i010();
  std::unique_ptr<NV12Buffer> get_nv12();
  rtc::scoped_refptr<webrtc::VideoFrameBuffer> get() const;

 protected:
  rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};

class PlanarYuvBuffer : public VideoFrameBuffer {
 public:
  explicit PlanarYuvBuffer(rtc::scoped_refptr<webrtc::PlanarYuvBuffer> buffer);

  int chroma_width() const;
  int chroma_height() const;

  int stride_y() const;
  int stride_u() const;
  int stride_v() const;

 private:
  webrtc::PlanarYuvBuffer* buffer() const;
};

class PlanarYuv8Buffer : public PlanarYuvBuffer {
 public:
  explicit PlanarYuv8Buffer(
      rtc::scoped_refptr<webrtc::PlanarYuv8Buffer> buffer);

  const uint8_t* data_y() const;
  const uint8_t* data_u() const;
  const uint8_t* data_v() const;

 private:
  webrtc::PlanarYuv8Buffer* buffer() const;
};

class PlanarYuv16BBuffer : public PlanarYuvBuffer {
 public:
  explicit PlanarYuv16BBuffer(
      rtc::scoped_refptr<webrtc::PlanarYuv16BBuffer> buffer);

  const uint16_t* data_y() const;
  const uint16_t* data_u() const;
  const uint16_t* data_v() const;

 private:
  webrtc::PlanarYuv16BBuffer* buffer() const;
};

class BiplanarYuvBuffer : public VideoFrameBuffer {
 public:
  explicit BiplanarYuvBuffer(
      rtc::scoped_refptr<webrtc::BiplanarYuvBuffer> buffer);

  int chroma_width() const;
  int chroma_height() const;

  int stride_y() const;
  int stride_uv() const;

 private:
  webrtc::BiplanarYuvBuffer* buffer() const;
};

class BiplanarYuv8Buffer : public BiplanarYuvBuffer {
 public:
  explicit BiplanarYuv8Buffer(
      rtc::scoped_refptr<webrtc::BiplanarYuv8Buffer> buffer);

  const uint8_t* data_y() const;
  const uint8_t* data_uv() const;

 private:
  webrtc::BiplanarYuv8Buffer* buffer() const;
};

class I420Buffer : public PlanarYuv8Buffer {
 public:
  explicit I420Buffer(rtc::scoped_refptr<webrtc::I420BufferInterface> buffer);
};

class I420ABuffer : public I420Buffer {
 public:
  explicit I420ABuffer(rtc::scoped_refptr<webrtc::I420ABufferInterface> buffer);
};

class I422Buffer : public PlanarYuv8Buffer {
 public:
  explicit I422Buffer(rtc::scoped_refptr<webrtc::I422BufferInterface> buffer);
};

class I444Buffer : public PlanarYuv8Buffer {
 public:
  explicit I444Buffer(rtc::scoped_refptr<webrtc::I444BufferInterface> buffer);
};

class I010Buffer : public PlanarYuv16BBuffer {
 public:
  explicit I010Buffer(rtc::scoped_refptr<webrtc::I010BufferInterface> buffer);
};

class NV12Buffer : public BiplanarYuv8Buffer {
 public:
  explicit NV12Buffer(rtc::scoped_refptr<webrtc::NV12BufferInterface> buffer);
};

static const VideoFrameBuffer* yuv_to_vfb(const PlanarYuvBuffer* yuv) {
  return yuv;
}

static const VideoFrameBuffer* biyuv_to_vfb(const BiplanarYuvBuffer* biyuv) {
  return biyuv;
}

static const PlanarYuvBuffer* yuv8_to_yuv(const PlanarYuv8Buffer* yuv8) {
  return yuv8;
}

static const PlanarYuvBuffer* yuv16b_to_yuv(const PlanarYuv16BBuffer* yuv16) {
  return yuv16;
}

static const BiplanarYuvBuffer* biyuv8_to_biyuv(
    const BiplanarYuv8Buffer* biyuv8) {
  return biyuv8;
}

static const PlanarYuv8Buffer* i420_to_yuv8(const I420Buffer* i420) {
  return i420;
}

static const PlanarYuv8Buffer* i420a_to_yuv8(const I420ABuffer* i420a) {
  return i420a;
}

static const PlanarYuv8Buffer* i422_to_yuv8(const I422Buffer* i422) {
  return i422;
}

static const PlanarYuv8Buffer* i444_to_yuv8(const I444Buffer* i444) {
  return i444;
}

static const PlanarYuv16BBuffer* i010_to_yuv16b(const I010Buffer* i010) {
  return i010;
}

static const BiplanarYuv8Buffer* nv12_to_biyuv8(const NV12Buffer* nv12) {
  return nv12;
}

}  // namespace livekit

#endif  // LIVEKIT_WEBRTC_VIDEO_FRAME_BUFFER_H
