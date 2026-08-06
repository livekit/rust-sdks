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

#include "nvidia_nv12_video_frame_buffer.h"

#include <api/make_ref_counted.h>
#include <api/video/i420_buffer.h>
#include <rtc_base/logging.h>
#include <third_party/libyuv/include/libyuv/convert.h>

#include <cstring>
#include <unistd.h>
#include <vector>

namespace livekit {
namespace {

bool CudaSucceeded(CUresult result, const char* operation) {
  if (result == CUDA_SUCCESS) {
    return true;
  }
  const char* error_name = nullptr;
  cuGetErrorName(result, &error_name);
  RTC_LOG(LS_ERROR) << operation << " failed: "
                    << (error_name ? error_name : "unknown CUDA error");
  return false;
}

class NvidiaCudaNv12RenderTarget final
    : public livekit_ffi::CudaNv12RenderTarget {
 public:
  explicit NvidiaCudaNv12RenderTarget(CUcontext context) : context_(context) {}

  ~NvidiaCudaNv12RenderTarget() override {
    if (context_ && CudaSucceeded(cuCtxPushCurrent(context_),
                                  "cuCtxPushCurrent")) {
      if (stream_) {
        cuStreamSynchronize(stream_);
        cuStreamDestroy(stream_);
      }
      if (device_ptr_) {
        cuMemFree(device_ptr_);
      }
      if (semaphore_) {
        cuDestroyExternalSemaphore(semaphore_);
      }
      if (memory_) {
        cuDestroyExternalMemory(memory_);
      }
      CUcontext popped = nullptr;
      cuCtxPopCurrent(&popped);
    }
  }

  bool Initialize(int memory_fd,
                  uint64_t allocation_size,
                  uint32_t destination_pitch,
                  uint64_t uv_offset,
                  int semaphore_fd,
                  int width,
                  int height) {
    if (memory_fd < 0 || semaphore_fd < 0 || allocation_size == 0 ||
        destination_pitch < static_cast<uint32_t>(width) || uv_offset == 0) {
      close(memory_fd);
      close(semaphore_fd);
      return false;
    }

    const uint64_t required_size =
        uv_offset + static_cast<uint64_t>(destination_pitch) *
                        static_cast<uint64_t>((height + 1) / 2);
    if (required_size > allocation_size) {
      close(memory_fd);
      close(semaphore_fd);
      return false;
    }

    if (!CudaSucceeded(cuCtxPushCurrent(context_), "cuCtxPushCurrent")) {
      close(memory_fd);
      close(semaphore_fd);
      return false;
    }

    CUDA_EXTERNAL_MEMORY_HANDLE_DESC memory_desc{};
    memory_desc.type = CU_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD;
    memory_desc.handle.fd = memory_fd;
    memory_desc.size = allocation_size;
    memory_desc.flags = CUDA_EXTERNAL_MEMORY_DEDICATED;
    CUresult result = cuImportExternalMemory(&memory_, &memory_desc);
    if (result != CUDA_SUCCESS) {
      close(memory_fd);
    }

    if (result == CUDA_SUCCESS) {
      CUDA_EXTERNAL_MEMORY_BUFFER_DESC buffer_desc{};
      buffer_desc.offset = 0;
      buffer_desc.size = allocation_size;
      result = cuExternalMemoryGetMappedBuffer(&device_ptr_, memory_,
                                               &buffer_desc);
    }

    CUDA_EXTERNAL_SEMAPHORE_HANDLE_DESC semaphore_desc{};
    if (result == CUDA_SUCCESS) {
      semaphore_desc.type = CU_EXTERNAL_SEMAPHORE_HANDLE_TYPE_OPAQUE_FD;
      semaphore_desc.handle.fd = semaphore_fd;
      result = cuImportExternalSemaphore(&semaphore_, &semaphore_desc);
      if (result != CUDA_SUCCESS) {
        close(semaphore_fd);
      }
    } else {
      close(semaphore_fd);
    }

    if (result == CUDA_SUCCESS) {
      result = cuStreamCreate(&stream_, CU_STREAM_NON_BLOCKING);
    }

    CUcontext popped = nullptr;
    cuCtxPopCurrent(&popped);
    if (!CudaSucceeded(result, "CUDA external render target import")) {
      return false;
    }

    allocation_size_ = allocation_size;
    destination_pitch_ = destination_pitch;
    uv_offset_ = uv_offset;
    width_ = width;
    height_ = height;
    return true;
  }

  bool CopyFrom(NvidiaNv12VideoFrameBuffer& frame) {
    if (frame.context() != context_ || frame.width() != width_ ||
        frame.height() != height_) {
      return false;
    }
    if (!CudaSucceeded(cuCtxPushCurrent(context_), "cuCtxPushCurrent")) {
      return false;
    }

    CUDA_MEMCPY2D copy{};
    copy.srcMemoryType = CU_MEMORYTYPE_DEVICE;
    copy.srcDevice = frame.device_frame();
    copy.srcPitch = frame.pitch();
    copy.dstMemoryType = CU_MEMORYTYPE_DEVICE;
    copy.dstDevice = device_ptr_;
    copy.dstPitch = destination_pitch_;
    copy.WidthInBytes = width_;
    copy.Height = height_;
    CUresult result = cuMemcpy2DAsync(&copy, stream_);

    if (result == CUDA_SUCCESS) {
      copy.srcDevice = frame.device_frame() +
                       static_cast<CUdeviceptr>(frame.pitch()) * height_;
      copy.dstDevice = device_ptr_ + uv_offset_;
      copy.Height = (height_ + 1) / 2;
      result = cuMemcpy2DAsync(&copy, stream_);
    }

    if (result == CUDA_SUCCESS) {
      CUDA_EXTERNAL_SEMAPHORE_SIGNAL_PARAMS signal_params{};
      result = cuSignalExternalSemaphoresAsync(&semaphore_, &signal_params, 1,
                                               stream_);
    }
    if (result == CUDA_SUCCESS) {
      // The source frame may be released as soon as this call returns. Wait
      // only for the short device-local copy and semaphore signal, never for
      // Vulkan rendering.
      result = cuStreamSynchronize(stream_);
    }

    CUcontext popped = nullptr;
    cuCtxPopCurrent(&popped);
    return CudaSucceeded(result, "CUDA NV12 render copy");
  }

 private:
  CUcontext context_ = nullptr;
  CUexternalMemory memory_ = nullptr;
  CUexternalSemaphore semaphore_ = nullptr;
  CUdeviceptr device_ptr_ = 0;
  CUstream stream_ = nullptr;
  uint64_t allocation_size_ = 0;
  uint32_t destination_pitch_ = 0;
  uint64_t uv_offset_ = 0;
  int width_ = 0;
  int height_ = 0;
};

}  // namespace

NvidiaNv12VideoFrameBuffer::NvidiaNv12VideoFrameBuffer(
    std::shared_ptr<NvDecoder> decoder,
    uint8_t* device_frame,
    CUcontext context,
    int width,
    int height,
    int pitch)
    : decoder_(std::move(decoder)),
      device_frame_(device_frame),
      context_(context),
      width_(width),
      height_(height),
      pitch_(pitch) {
  CUdevice device = 0;
  if (!CudaSucceeded(cuCtxPushCurrent(context_), "cuCtxPushCurrent")) {
    return;
  }
  CUresult device_result = cuCtxGetDevice(&device);
  CUuuid uuid{};
  CUresult uuid_result = device_result == CUDA_SUCCESS
                             ? cuDeviceGetUuid(&uuid, device)
                             : device_result;
  CUcontext popped = nullptr;
  cuCtxPopCurrent(&popped);
  if (CudaSucceeded(uuid_result, "cuDeviceGetUuid")) {
    std::memcpy(device_uuid_.data(), uuid.bytes, device_uuid_.size());
  }
}

NvidiaNv12VideoFrameBuffer::~NvidiaNv12VideoFrameBuffer() {
  if (decoder_ && device_frame_) {
    decoder_->UnlockFrame(&device_frame_);
  }
}

webrtc::VideoFrameBuffer::Type NvidiaNv12VideoFrameBuffer::type() const {
  return Type::kNative;
}

int NvidiaNv12VideoFrameBuffer::width() const {
  return width_;
}

int NvidiaNv12VideoFrameBuffer::height() const {
  return height_;
}

webrtc::scoped_refptr<webrtc::I420BufferInterface>
NvidiaNv12VideoFrameBuffer::ToI420() {
  const int chroma_height = (height_ + 1) / 2;
  std::vector<uint8_t> host_nv12(
      static_cast<size_t>(pitch_) * (height_ + chroma_height));

  if (!CudaSucceeded(cuCtxPushCurrent(context_), "cuCtxPushCurrent")) {
    return nullptr;
  }
  CUresult copy_result = cuMemcpyDtoH(host_nv12.data(), device_frame(),
                                     host_nv12.size());
  CUcontext popped = nullptr;
  cuCtxPopCurrent(&popped);
  if (!CudaSucceeded(copy_result, "cuMemcpyDtoH")) {
    return nullptr;
  }

  auto i420 = webrtc::I420Buffer::Create(width_, height_);
  int result = libyuv::NV12ToI420(
      host_nv12.data(), pitch_, host_nv12.data() + pitch_ * height_, pitch_,
      i420->MutableDataY(), i420->StrideY(), i420->MutableDataU(),
      i420->StrideU(), i420->MutableDataV(), i420->StrideV(), width_,
      height_);
  if (result != 0) {
    RTC_LOG(LS_ERROR) << "NvidiaNv12VideoFrameBuffer::ToI420 failed: "
                      << result;
    return nullptr;
  }
  return i420;
}

webrtc::scoped_refptr<webrtc::VideoFrameBuffer>
NvidiaNv12VideoFrameBuffer::CropAndScale(int offset_x,
                                         int offset_y,
                                         int crop_width,
                                         int crop_height,
                                         int scaled_width,
                                         int scaled_height) {
  if (offset_x == 0 && offset_y == 0 && crop_width == width_ &&
      crop_height == height_ && scaled_width == width_ &&
      scaled_height == height_) {
    return webrtc::scoped_refptr<webrtc::VideoFrameBuffer>(this);
  }
  auto i420 = ToI420();
  return i420 ? i420->CropAndScale(offset_x, offset_y, crop_width, crop_height,
                                  scaled_width, scaled_height)
              : nullptr;
}

NvidiaNv12VideoFrameBuffer* NvidiaNv12VideoFrameBuffer::FromNative(
    webrtc::VideoFrameBuffer* buffer) {
  if (!buffer || buffer->type() != webrtc::VideoFrameBuffer::Type::kNative) {
    return nullptr;
  }
  return dynamic_cast<NvidiaNv12VideoFrameBuffer*>(buffer);
}

std::unique_ptr<livekit_ffi::CudaNv12RenderTarget>
CreateCudaNv12RenderTarget(NvidiaNv12VideoFrameBuffer& frame,
                           int memory_fd,
                           uint64_t allocation_size,
                           uint32_t destination_pitch,
                           uint64_t uv_offset,
                           int semaphore_fd) {
  auto target =
      std::make_unique<NvidiaCudaNv12RenderTarget>(frame.context());
  if (!target->Initialize(memory_fd, allocation_size, destination_pitch,
                          uv_offset, semaphore_fd, frame.width(),
                          frame.height())) {
    return nullptr;
  }
  return target;
}

bool CopyCudaNv12ToRenderTarget(
    NvidiaNv12VideoFrameBuffer& frame,
    livekit_ffi::CudaNv12RenderTarget& target) {
  auto* nvidia_target = dynamic_cast<NvidiaCudaNv12RenderTarget*>(&target);
  return nvidia_target && nvidia_target->CopyFrom(frame);
}

}  // namespace livekit
