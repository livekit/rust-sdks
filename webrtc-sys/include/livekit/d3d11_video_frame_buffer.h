/*
 * Copyright 2026 LiveKit, Inc.
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

#include <cstdint>
#include <memory>
#include <span>
#include <string>

#include "api/scoped_refptr.h"
#include "api/video/video_frame_buffer.h"

struct ID3D11Texture2D;

namespace livekit {

// A VideoFrameBuffer backed by a BGRA Direct3D 11 texture, as a screen
// capturer hands them out. Reports Type::kNative, so a captured frame reaches
// the encoder as the texture itself: a hardware encoder can read it on the GPU
// with no CPU readback (see FromNative).
//
// An encoder that wants memory (VP8, OpenH264) maps it through
// GetMappedFrameBuffer or ToI420. That converts BGRA to NV12 on the GPU with
// the D3D11 video processor (BT.601, limited range, as libyuv's ARGBToI420),
// scaled to the requested size, and reads the NV12 back once. The readback is
// cached for the captured frame: further mappings of the same region at the
// same or a smaller size (simulcast layers, a second sink) scale the cached
// NV12 on the CPU instead of reading the texture again. A device without the
// video processor (created without D3D11_CREATE_DEVICE_VIDEO_SUPPORT) reads
// the BGRA back instead and converts with libyuv.
//
// CropAndScale and Scale return views on the same texture and never touch the
// GPU, so adaptation and simulcast keep the frame native.
//
// The texture's device must have multithread protection on
// (ID3D11Multithread::SetMultithreadProtected): the readback runs on the
// encoder's thread, on the device's immediate context.
class D3D11VideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  struct Rect {
    int x = 0;
    int y = 0;
    int width = 0;
    int height = 0;
    bool operator==(const Rect&) const = default;
  };

  // Wraps the top-left `width` x `height` of `texture`, which must be
  // DXGI_FORMAT_B8G8R8A8_UNORM. Takes a reference on the texture. The caller's
  // `keep_alive` is released with the last view of the frame: it is what keeps
  // the texture's contents from being overwritten (a capture pool's slot).
  // Returns null for a null texture, another format, or a size the texture
  // does not have.
  static webrtc::scoped_refptr<D3D11VideoFrameBuffer> Create(
      ID3D11Texture2D* texture,
      int width,
      int height,
      std::shared_ptr<void> keep_alive);

  // webrtc::VideoFrameBuffer
  Type type() const override;
  int width() const override;
  int height() const override;
  webrtc::scoped_refptr<webrtc::I420BufferInterface> ToI420() override;
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> CropAndScale(
      int offset_x,
      int offset_y,
      int crop_width,
      int crop_height,
      int scaled_width,
      int scaled_height) override;
  webrtc::scoped_refptr<webrtc::VideoFrameBuffer> GetMappedFrameBuffer(
      std::span<Type> types) override;
  std::string storage_representation() const override;

  // The texture (no reference added) and the part of it this buffer shows.
  // The picture is source_rect() of texture() scaled to width() x height().
  ID3D11Texture2D* texture() const;
  Rect source_rect() const { return source_rect_; }

  // NV12 of this view: the one GPU readback, or a CPU scale of it.
  webrtc::scoped_refptr<webrtc::NV12BufferInterface> ToNV12();

  // Null unless `buffer` is a D3D11VideoFrameBuffer.
  static D3D11VideoFrameBuffer* FromNative(webrtc::VideoFrameBuffer* buffer);

  // GPU-to-CPU readbacks done by every D3D11VideoFrameBuffer in the process.
  // For tests and diagnostics.
  static uint64_t readback_count();

  struct Frame;

 protected:
  D3D11VideoFrameBuffer(std::shared_ptr<Frame> frame,
                        Rect source_rect,
                        int width,
                        int height);
  ~D3D11VideoFrameBuffer() override;

 private:
  std::shared_ptr<Frame> frame_;
  Rect source_rect_;
  int width_;
  int height_;
};

}  // namespace livekit
