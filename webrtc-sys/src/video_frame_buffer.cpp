#include "livekit/video_frame_buffer.h"

namespace livekit {

VideoFrameBuffer::VideoFrameBuffer(
    rtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer)
    : buffer_(std::move(buffer)) {}

VideoFrameBufferType VideoFrameBuffer::buffer_type() const {
  return static_cast<VideoFrameBufferType>(buffer_->type());
}

int VideoFrameBuffer::width() const {
  return buffer_->width();
}

int VideoFrameBuffer::height() const {
  return buffer_->height();
}

std::unique_ptr<I420Buffer> VideoFrameBuffer::to_i420() {
  return std::make_unique<I420Buffer>(buffer_->ToI420());
}

// const_cast is valid here because we take the ownership on the rust side
std::unique_ptr<I420Buffer> VideoFrameBuffer::get_i420() {
  return std::make_unique<I420Buffer>(
      rtc::scoped_refptr<webrtc::I420BufferInterface>(
          const_cast<webrtc::I420BufferInterface*>(buffer_->GetI420())));
}

std::unique_ptr<I420ABuffer> VideoFrameBuffer::get_i420a() {
  return std::make_unique<I420ABuffer>(
      rtc::scoped_refptr<webrtc::I420ABufferInterface>(
          const_cast<webrtc::I420ABufferInterface*>(buffer_->GetI420A())));
}

std::unique_ptr<I422Buffer> VideoFrameBuffer::get_i422() {
  return std::make_unique<I422Buffer>(
      rtc::scoped_refptr<webrtc::I422BufferInterface>(
          const_cast<webrtc::I422BufferInterface*>(buffer_->GetI422())));
}

std::unique_ptr<I444Buffer> VideoFrameBuffer::get_i444() {
  return std::make_unique<I444Buffer>(
      rtc::scoped_refptr<webrtc::I444BufferInterface>(
          const_cast<webrtc::I444BufferInterface*>(buffer_->GetI444())));
}

std::unique_ptr<I010Buffer> VideoFrameBuffer::get_i010() {
  return std::make_unique<I010Buffer>(
      rtc::scoped_refptr<webrtc::I010BufferInterface>(
          const_cast<webrtc::I010BufferInterface*>(buffer_->GetI010())));
}

std::unique_ptr<NV12Buffer> VideoFrameBuffer::get_nv12() {
  return std::make_unique<NV12Buffer>(
      rtc::scoped_refptr<webrtc::NV12BufferInterface>(
          const_cast<webrtc::NV12BufferInterface*>(buffer_->GetNV12())));
}

rtc::scoped_refptr<webrtc::VideoFrameBuffer> VideoFrameBuffer::get() const {
  return buffer_;
}

PlanarYuvBuffer::PlanarYuvBuffer(
    rtc::scoped_refptr<webrtc::PlanarYuvBuffer> buffer)
    : VideoFrameBuffer(buffer) {}

int PlanarYuvBuffer::chroma_width() const {
  return buffer()->ChromaWidth();
}

int PlanarYuvBuffer::chroma_height() const {
  return buffer()->ChromaHeight();
}

int PlanarYuvBuffer::stride_y() const {
  return buffer()->StrideY();
}

int PlanarYuvBuffer::stride_u() const {
  return buffer()->StrideU();
}

int PlanarYuvBuffer::stride_v() const {
  return buffer()->StrideV();
}

webrtc::PlanarYuvBuffer* PlanarYuvBuffer::buffer() const {
  return static_cast<webrtc::PlanarYuvBuffer*>(buffer_.get());
}

PlanarYuv8Buffer::PlanarYuv8Buffer(
    rtc::scoped_refptr<webrtc::PlanarYuv8Buffer> buffer)
    : PlanarYuvBuffer(buffer) {}

const uint8_t* PlanarYuv8Buffer::data_y() const {
  return buffer()->DataY();
}

const uint8_t* PlanarYuv8Buffer::data_u() const {
  return buffer()->DataU();
}

const uint8_t* PlanarYuv8Buffer::data_v() const {
  return buffer()->DataV();
}

webrtc::PlanarYuv8Buffer* PlanarYuv8Buffer::buffer() const {
  return static_cast<webrtc::PlanarYuv8Buffer*>(buffer_.get());
}

PlanarYuv16BBuffer::PlanarYuv16BBuffer(
    rtc::scoped_refptr<webrtc::PlanarYuv16BBuffer> buffer)
    : PlanarYuvBuffer(buffer) {}

const uint16_t* PlanarYuv16BBuffer::data_y() const {
  return buffer()->DataY();
}

const uint16_t* PlanarYuv16BBuffer::data_u() const {
  return buffer()->DataU();
}

const uint16_t* PlanarYuv16BBuffer::data_v() const {
  return buffer()->DataV();
}

webrtc::PlanarYuv16BBuffer* PlanarYuv16BBuffer::buffer() const {
  return static_cast<webrtc::PlanarYuv16BBuffer*>(buffer_.get());
}

BiplanarYuvBuffer::BiplanarYuvBuffer(
    rtc::scoped_refptr<webrtc::BiplanarYuvBuffer> buffer)
    : VideoFrameBuffer(buffer) {}

int BiplanarYuvBuffer::chroma_width() const {
  return buffer()->ChromaWidth();
}

int BiplanarYuvBuffer::chroma_height() const {
  return buffer()->ChromaHeight();
}

int BiplanarYuvBuffer::stride_y() const {
  return buffer()->StrideY();
}

int BiplanarYuvBuffer::stride_uv() const {
  return buffer()->StrideUV();
}

webrtc::BiplanarYuvBuffer* BiplanarYuvBuffer::buffer() const {
  return static_cast<webrtc::BiplanarYuvBuffer*>(buffer_.get());
}

BiplanarYuv8Buffer::BiplanarYuv8Buffer(
    rtc::scoped_refptr<webrtc::BiplanarYuv8Buffer> buffer)
    : BiplanarYuvBuffer(buffer) {}

const uint8_t* BiplanarYuv8Buffer::data_y() const {
  return buffer()->DataY();
}

const uint8_t* BiplanarYuv8Buffer::data_uv() const {
  return buffer()->DataUV();
}

webrtc::BiplanarYuv8Buffer* BiplanarYuv8Buffer::buffer() const {
  return static_cast<webrtc::BiplanarYuv8Buffer*>(buffer_.get());
}

std::unique_ptr<I420Buffer> create_i420_buffer(int width, int height) {
  return std::make_unique<I420Buffer>(
      webrtc::I420Buffer::Create(width, height));
}

I420Buffer::I420Buffer(rtc::scoped_refptr<webrtc::I420BufferInterface> buffer)
    : PlanarYuv8Buffer(buffer) {}

I420ABuffer::I420ABuffer(
    rtc::scoped_refptr<webrtc::I420ABufferInterface> buffer)
    : I420Buffer(buffer) {}

I422Buffer::I422Buffer(rtc::scoped_refptr<webrtc::I422BufferInterface> buffer)
    : PlanarYuv8Buffer(buffer) {}

I444Buffer::I444Buffer(rtc::scoped_refptr<webrtc::I444BufferInterface> buffer)
    : PlanarYuv8Buffer(buffer) {}

I010Buffer::I010Buffer(rtc::scoped_refptr<webrtc::I010BufferInterface> buffer)
    : PlanarYuv16BBuffer(buffer) {}

NV12Buffer::NV12Buffer(rtc::scoped_refptr<webrtc::NV12BufferInterface> buffer)
    : BiplanarYuv8Buffer(buffer) {}

}  // namespace livekit
