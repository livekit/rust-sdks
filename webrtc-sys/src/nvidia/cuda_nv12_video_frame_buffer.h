#ifndef WEBRTC_NVIDIA_CUDA_NV12_VIDEO_FRAME_BUFFER_H_
#define WEBRTC_NVIDIA_CUDA_NV12_VIDEO_FRAME_BUFFER_H_

#include <cuda.h>

#include <cstdint>
#include <string>

#include "api/scoped_refptr.h"
#include "api/video/i420_buffer.h"
#include "api/video/video_frame_buffer.h"

namespace webrtc {

class NvidiaCudaNv12VideoFrameBuffer : public VideoFrameBuffer {
 public:
  static scoped_refptr<NvidiaCudaNv12VideoFrameBuffer> Create(
      CUcontext context,
      CUdeviceptr source_device_ptr,
      uint32_t source_pitch,
      uint32_t width,
      uint32_t height);

  NvidiaCudaNv12VideoFrameBuffer(CUcontext context,
                                 CUdeviceptr device_ptr,
                                 uint32_t pitch,
                                 uint32_t width,
                                 uint32_t height);
  ~NvidiaCudaNv12VideoFrameBuffer() override;

  Type type() const override;
  int width() const override;
  int height() const override;
  scoped_refptr<I420BufferInterface> ToI420() override;
  std::string storage_representation() const override;

  CUcontext cuda_context() const { return context_; }
  CUdeviceptr device_ptr() const { return device_ptr_; }
  uint32_t pitch() const { return pitch_; }

 private:
  CUcontext context_;
  CUdeviceptr device_ptr_;
  uint32_t pitch_;
  uint32_t width_;
  uint32_t height_;
};

bool CopyDeviceNv12ToI420(CUcontext context,
                          CUdeviceptr source_device_ptr,
                          uint32_t source_pitch,
                          uint32_t width,
                          uint32_t height,
                          I420Buffer* destination);

bool CopyNvidiaCudaNv12ToExternalImages(CUcontext context,
                                        CUdeviceptr source_device_ptr,
                                        uint32_t source_pitch,
                                        uint32_t width,
                                        uint32_t height,
                                        int32_t y_fd,
                                        uint64_t y_allocation_size,
                                        int32_t uv_fd,
                                        uint64_t uv_allocation_size);

}  // namespace webrtc

#endif  // WEBRTC_NVIDIA_CUDA_NV12_VIDEO_FRAME_BUFFER_H_
