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

#include <api/video/video_frame_buffer.h>
#include <cuda.h>

#include <array>
#include <cstdint>
#include <memory>

#include "NvDecoder/NvDecoder.h"
#include "livekit/video_frame_buffer.h"

namespace livekit {

// A native NV12 frame stored in CUDA device memory owned by NvDecoder.
class NvidiaNv12VideoFrameBuffer : public webrtc::VideoFrameBuffer {
 public:
  NvidiaNv12VideoFrameBuffer(std::shared_ptr<NvDecoder> decoder,
                             uint8_t* device_frame,
                             CUcontext context,
                             int width,
                             int height,
                             int pitch);
  ~NvidiaNv12VideoFrameBuffer() override;

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

  int pitch() const { return pitch_; }
  CUcontext context() const { return context_; }
  CUdeviceptr device_frame() const {
    return reinterpret_cast<CUdeviceptr>(device_frame_);
  }
  const std::array<uint8_t, 16>& device_uuid() const { return device_uuid_; }

  static NvidiaNv12VideoFrameBuffer* FromNative(
      webrtc::VideoFrameBuffer* buffer);

 private:
  std::shared_ptr<NvDecoder> decoder_;
  uint8_t* device_frame_;
  CUcontext context_;
  int width_;
  int height_;
  int pitch_;
  std::array<uint8_t, 16> device_uuid_{};
};

std::unique_ptr<livekit_ffi::CudaNv12RenderTarget>
CreateCudaNv12RenderTarget(NvidiaNv12VideoFrameBuffer& frame,
                           int memory_fd,
                           uint64_t allocation_size,
                           uint32_t destination_pitch,
                           uint64_t uv_offset,
                           int semaphore_fd);

bool CopyCudaNv12ToRenderTarget(
    NvidiaNv12VideoFrameBuffer& frame,
    livekit_ffi::CudaNv12RenderTarget& target);

}  // namespace livekit
