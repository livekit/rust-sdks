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

#pragma once

#include <memory>

#include "api/scoped_refptr.h"
#include "api/video/i010_buffer.h"
#include "api/video/i420_buffer.h"
#include "api/video/i422_buffer.h"
#include "api/video/i444_buffer.h"
#include "api/video/nv12_buffer.h"
#include "api/video/video_frame_buffer.h"
#include "livekit_rtc/capi.h"

#ifdef __APPLE__
#include <CoreVideo/CoreVideo.h>
namespace livekit {
typedef __CVBuffer PlatformImageBuffer;
}  // namespace livekit
#else
namespace livekit {
typedef void PlatformImageBuffer;
}  // namespace livekit
#endif

namespace livekit {

class VideoFrameBuffer;
class PlanarYuvBuffer;
class PlanarYuv8Buffer;
class PlanarYuv16BBuffer;
class BiplanarYuvBuffer;
class BiplanarYuv8Buffer;
class I420Buffer;
class I420ABuffer;
class I422Buffer;
class I444Buffer;
class I010Buffer;
class NV12Buffer;

class VideoFrameBuffer : public webrtc::RefCountInterface {
 public:
  explicit VideoFrameBuffer(
      webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer);

  lkVideoBufferType buffer_type() const;

  unsigned int width() const;
  unsigned int height() const;

  webrtc::scoped_refptr<I420Buffer> to_i420() const;

  // Requires ownership
  webrtc::scoped_refptr<I420Buffer> get_i420();
  webrtc::scoped_refptr<I420ABuffer> get_i420a();
  webrtc::scoped_refptr<I422Buffer> get_i422();
  webrtc::scoped_refptr<I444Buffer> get_i444();
  webrtc::scoped_refptr<I010Buffer> get_i010();
  webrtc::scoped_refptr<NV12Buffer> get_nv12();
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> get() const;

 protected:
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> buffer_;
};

class PlanarYuvBuffer : public VideoFrameBuffer {
 public:
  explicit PlanarYuvBuffer(
      webrtc::scoped_refptr<webrtc::PlanarYuvBuffer> buffer);

  unsigned int chroma_width() const;
  unsigned int chroma_height() const;

  unsigned int stride_y() const;
  unsigned int stride_u() const;
  unsigned int stride_v() const;

 private:
  webrtc::PlanarYuvBuffer* buffer() const;
};

class PlanarYuv8Buffer : public PlanarYuvBuffer {
 public:
  explicit PlanarYuv8Buffer(
      webrtc::scoped_refptr<webrtc::PlanarYuv8Buffer> buffer);

  const uint8_t* data_y() const;
  const uint8_t* data_u() const;
  const uint8_t* data_v() const;

 private:
  webrtc::PlanarYuv8Buffer* buffer() const;
};

class PlanarYuv16BBuffer : public PlanarYuvBuffer {
 public:
  explicit PlanarYuv16BBuffer(
      webrtc::scoped_refptr<webrtc::PlanarYuv16BBuffer> buffer);

  const uint16_t* data_y() const;
  const uint16_t* data_u() const;
  const uint16_t* data_v() const;

 private:
  webrtc::PlanarYuv16BBuffer* buffer() const;
};

class BiplanarYuvBuffer : public VideoFrameBuffer {
 public:
  explicit BiplanarYuvBuffer(
      webrtc::scoped_refptr<webrtc::BiplanarYuvBuffer> buffer);

  unsigned int chroma_width() const;
  unsigned int chroma_height() const;

  unsigned int stride_y() const;
  unsigned int stride_uv() const;

 private:
  webrtc::BiplanarYuvBuffer* buffer() const;
};

class BiplanarYuv8Buffer : public BiplanarYuvBuffer {
 public:
  explicit BiplanarYuv8Buffer(
      webrtc::scoped_refptr<webrtc::BiplanarYuv8Buffer> buffer);

  const uint8_t* data_y() const;
  const uint8_t* data_uv() const;

 private:
  webrtc::BiplanarYuv8Buffer* buffer() const;
};

class I420Buffer : public PlanarYuv8Buffer {
 public:
  explicit I420Buffer(
      webrtc::scoped_refptr<webrtc::I420BufferInterface> buffer);

  webrtc::scoped_refptr<I420Buffer> scale(int scaled_width,
                                          int scaled_height) const;

 private:
  webrtc::I420BufferInterface* buffer() const;
};

class I420ABuffer : public I420Buffer {
 public:
  explicit I420ABuffer(
      webrtc::scoped_refptr<webrtc::I420ABufferInterface> buffer);

  unsigned int stride_a() const;
  const uint8_t* data_a() const;

  webrtc::scoped_refptr<I420ABuffer> scale(int scaled_width,
                                           int scaled_height) const;

 private:
  webrtc::I420ABufferInterface* buffer() const;
};

class I422Buffer : public PlanarYuv8Buffer {
 public:
  explicit I422Buffer(
      webrtc::scoped_refptr<webrtc::I422BufferInterface> buffer);

  webrtc::scoped_refptr<I422Buffer> scale(int scaled_width,
                                          int scaled_height) const;

 private:
  webrtc::I422BufferInterface* buffer() const;
};

class I444Buffer : public PlanarYuv8Buffer {
 public:
  explicit I444Buffer(
      webrtc::scoped_refptr<webrtc::I444BufferInterface> buffer);

  webrtc::scoped_refptr<I444Buffer> scale(int scaled_width,
                                          int scaled_height) const;

 private:
  webrtc::I444BufferInterface* buffer() const;
};

class I010Buffer : public PlanarYuv16BBuffer {
 public:
  explicit I010Buffer(
      webrtc::scoped_refptr<webrtc::I010BufferInterface> buffer);

  webrtc::scoped_refptr<I010Buffer> scale(int scaled_width,
                                          int scaled_height) const;

 private:
  webrtc::I010BufferInterface* buffer() const;
};

class NV12Buffer : public BiplanarYuv8Buffer {
 public:
  explicit NV12Buffer(
      webrtc::scoped_refptr<webrtc::NV12BufferInterface> buffer);

  webrtc::scoped_refptr<NV12Buffer> scale(int scaled_width,
                                          int scaled_height) const;

 private:
  webrtc::NV12BufferInterface* buffer() const;
};

webrtc::scoped_refptr<I420Buffer> copy_i420_buffer(
    const webrtc::scoped_refptr<I420Buffer>& i420);
webrtc::scoped_refptr<I420Buffer> new_i420_buffer(
    int width, int height, int stride_y, int stride_u, int stride_v);
webrtc::scoped_refptr<I422Buffer> new_i422_buffer(
    int width, int height, int stride_y, int stride_u, int stride_v);
webrtc::scoped_refptr<I444Buffer> new_i444_buffer(
    int width, int height, int stride_y, int stride_u, int stride_v);
webrtc::scoped_refptr<I010Buffer> new_i010_buffer(
    int width, int height, int stride_y, int stride_u, int stride_v);
webrtc::scoped_refptr<NV12Buffer> new_nv12_buffer(int width,
                                                  int height,
                                                  int stride_y,
                                                  int stride_uv);

#if defined(__APPLE__)
webrtc::scoped_refptr<VideoFrameBuffer>
    new_native_buffer_from_platform_image_buffer(CVPixelBufferRef pixelBuffer);
CVPixelBufferRef native_buffer_to_platform_image_buffer(
    const webrtc::scoped_refptr<VideoFrameBuffer>& buffer);
#else
webrtc::scoped_refptr<VideoFrameBuffer>
    new_native_buffer_from_platform_image_buffer(PlatformImageBuffer* buffer);
PlatformImageBuffer* native_buffer_to_platform_image_buffer(
    const webrtc::scoped_refptr<VideoFrameBuffer>&);
#endif

}  // namespace livekit
