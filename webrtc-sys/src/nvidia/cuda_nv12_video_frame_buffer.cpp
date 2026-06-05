#include "cuda_nv12_video_frame_buffer.h"

#include <third_party/libyuv/include/libyuv/convert.h>

#include <algorithm>
#include <vector>
#include <unistd.h>

#include "api/make_ref_counted.h"
#include "rtc_base/logging.h"

namespace webrtc {
namespace {

uint32_t ChromaHeight(uint32_t height) {
  return (height + 1) / 2;
}

uint32_t ChromaWidthBytes(uint32_t width) {
  return ((width + 1) / 2) * 2;
}

bool CheckCuda(CUresult result, const char* call) {
  if (result == CUDA_SUCCESS) {
    return true;
  }

  const char* name = nullptr;
  cuGetErrorName(result, &name);
  RTC_LOG(LS_WARNING) << call << " failed: "
                      << (name == nullptr ? "unknown" : name) << " ("
                      << result << ")";
  return false;
}

bool PushContext(CUcontext context) {
  return CheckCuda(cuCtxPushCurrent(context), "cuCtxPushCurrent");
}

void PopContext() {
  CUcontext popped = nullptr;
  CheckCuda(cuCtxPopCurrent(&popped), "cuCtxPopCurrent");
}

bool CopyDevicePlaneToDevice(CUdeviceptr dst,
                             uint32_t dst_pitch,
                             CUdeviceptr src,
                             uint32_t src_pitch,
                             uint32_t width_bytes,
                             uint32_t height) {
  CUDA_MEMCPY2D copy = {};
  copy.srcMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.srcDevice = src;
  copy.srcPitch = src_pitch;
  copy.dstMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.dstDevice = dst;
  copy.dstPitch = dst_pitch;
  copy.WidthInBytes = width_bytes;
  copy.Height = height;
  return CheckCuda(cuMemcpy2D(&copy), "cuMemcpy2D");
}

bool CopyDevicePlaneToHost(uint8_t* dst,
                           uint32_t dst_pitch,
                           CUdeviceptr src,
                           uint32_t src_pitch,
                           uint32_t width_bytes,
                           uint32_t height) {
  CUDA_MEMCPY2D copy = {};
  copy.srcMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.srcDevice = src;
  copy.srcPitch = src_pitch;
  copy.dstMemoryType = CU_MEMORYTYPE_HOST;
  copy.dstHost = dst;
  copy.dstPitch = dst_pitch;
  copy.WidthInBytes = width_bytes;
  copy.Height = height;
  return CheckCuda(cuMemcpy2D(&copy), "cuMemcpy2D");
}

bool CopyDevicePlaneToArray(CUarray dst,
                            CUdeviceptr src,
                            uint32_t src_pitch,
                            uint32_t width_bytes,
                            uint32_t height) {
  CUDA_MEMCPY2D copy = {};
  copy.srcMemoryType = CU_MEMORYTYPE_DEVICE;
  copy.srcDevice = src;
  copy.srcPitch = src_pitch;
  copy.dstMemoryType = CU_MEMORYTYPE_ARRAY;
  copy.dstArray = dst;
  copy.WidthInBytes = width_bytes;
  copy.Height = height;
  return CheckCuda(cuMemcpy2D(&copy), "cuMemcpy2D");
}

bool CopyPlaneToExternalImage(CUdeviceptr source_device_ptr,
                              uint32_t source_pitch,
                              uint32_t width_texels,
                              uint32_t height,
                              uint32_t channels,
                              uint32_t width_bytes,
                              int32_t fd,
                              uint64_t allocation_size) {
  CUDA_EXTERNAL_MEMORY_HANDLE_DESC memory_desc = {};
  memory_desc.type = CU_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD;
  memory_desc.handle.fd = fd;
  memory_desc.size = allocation_size;
  memory_desc.flags = CUDA_EXTERNAL_MEMORY_DEDICATED;

  CUexternalMemory external_memory = nullptr;
  if (!CheckCuda(cuImportExternalMemory(&external_memory, &memory_desc),
                 "cuImportExternalMemory")) {
    close(fd);
    return false;
  }

  CUDA_EXTERNAL_MEMORY_MIPMAPPED_ARRAY_DESC array_desc = {};
  array_desc.offset = 0;
  array_desc.numLevels = 1;
  array_desc.arrayDesc.Width = width_texels;
  array_desc.arrayDesc.Height = height;
  array_desc.arrayDesc.Depth = 0;
  array_desc.arrayDesc.Format = CU_AD_FORMAT_UNSIGNED_INT8;
  array_desc.arrayDesc.NumChannels = channels;
  array_desc.arrayDesc.Flags = 0;

  CUmipmappedArray mipmapped_array = nullptr;
  bool ok = CheckCuda(cuExternalMemoryGetMappedMipmappedArray(
                          &mipmapped_array, external_memory, &array_desc),
                      "cuExternalMemoryGetMappedMipmappedArray");
  if (ok) {
    CUarray array = nullptr;
    ok = CheckCuda(cuMipmappedArrayGetLevel(&array, mipmapped_array, 0),
                   "cuMipmappedArrayGetLevel");
    if (ok) {
      ok = CopyDevicePlaneToArray(array, source_device_ptr, source_pitch,
                                  width_bytes, height);
    }
  }

  if (mipmapped_array != nullptr) {
    CheckCuda(cuMipmappedArrayDestroy(mipmapped_array),
              "cuMipmappedArrayDestroy");
  }
  CheckCuda(cuDestroyExternalMemory(external_memory), "cuDestroyExternalMemory");
  return ok;
}

}  // namespace

scoped_refptr<NvidiaCudaNv12VideoFrameBuffer>
NvidiaCudaNv12VideoFrameBuffer::Create(CUcontext context,
                                       CUdeviceptr source_device_ptr,
                                       uint32_t source_pitch,
                                       uint32_t width,
                                       uint32_t height) {
  const uint32_t uv_height = ChromaHeight(height);

  if (!PushContext(context)) {
    return nullptr;
  }

  CUdeviceptr device_ptr = 0;
  size_t pitch = 0;
  const uint32_t allocation_width_bytes =
      std::max(width, ChromaWidthBytes(width));
  bool ok = CheckCuda(cuMemAllocPitch(&device_ptr, &pitch, allocation_width_bytes,
                                      height + uv_height, 16),
                      "cuMemAllocPitch");
  if (ok) {
    ok = CopyDevicePlaneToDevice(device_ptr, pitch, source_device_ptr,
                                 source_pitch, width, height);
  }
  if (ok) {
    ok = CopyDevicePlaneToDevice(
        device_ptr + pitch * height, pitch,
        source_device_ptr + source_pitch * height, source_pitch,
        ChromaWidthBytes(width), uv_height);
  }

  PopContext();

  if (!ok) {
    if (device_ptr != 0) {
      PushContext(context);
      CheckCuda(cuMemFree(device_ptr), "cuMemFree");
      PopContext();
    }
    return nullptr;
  }

  return make_ref_counted<NvidiaCudaNv12VideoFrameBuffer>(
      context, device_ptr, static_cast<uint32_t>(pitch), width, height);
}

NvidiaCudaNv12VideoFrameBuffer::NvidiaCudaNv12VideoFrameBuffer(
    CUcontext context,
    CUdeviceptr device_ptr,
    uint32_t pitch,
    uint32_t width,
    uint32_t height)
    : context_(context),
      device_ptr_(device_ptr),
      pitch_(pitch),
      width_(width),
      height_(height) {}

NvidiaCudaNv12VideoFrameBuffer::~NvidiaCudaNv12VideoFrameBuffer() {
  if (device_ptr_ == 0) {
    return;
  }
  if (PushContext(context_)) {
    CheckCuda(cuMemFree(device_ptr_), "cuMemFree");
    PopContext();
  }
}

VideoFrameBuffer::Type NvidiaCudaNv12VideoFrameBuffer::type() const {
  return Type::kNative;
}

int NvidiaCudaNv12VideoFrameBuffer::width() const {
  return width_;
}

int NvidiaCudaNv12VideoFrameBuffer::height() const {
  return height_;
}

scoped_refptr<I420BufferInterface> NvidiaCudaNv12VideoFrameBuffer::ToI420() {
  scoped_refptr<I420Buffer> i420 = I420Buffer::Create(width_, height_);
  if (!CopyDeviceNv12ToI420(context_, device_ptr_, pitch_, width_, height_,
                            i420.get())) {
    return nullptr;
  }
  return i420;
}

std::string NvidiaCudaNv12VideoFrameBuffer::storage_representation() const {
  return "NVIDIA CUDA NV12";
}

bool CopyDeviceNv12ToI420(CUcontext context,
                          CUdeviceptr source_device_ptr,
                          uint32_t source_pitch,
                          uint32_t width,
                          uint32_t height,
                          I420Buffer* destination) {
  if (destination == nullptr) {
    return false;
  }

  const uint32_t uv_height = ChromaHeight(height);
  const uint32_t uv_width_bytes = ChromaWidthBytes(width);
  std::vector<uint8_t> y(width * height);
  std::vector<uint8_t> uv(uv_width_bytes * uv_height);

  if (!PushContext(context)) {
    return false;
  }
  bool ok = CopyDevicePlaneToHost(y.data(), width, source_device_ptr,
                                  source_pitch, width, height);
  if (ok) {
    ok = CopyDevicePlaneToHost(uv.data(), uv_width_bytes,
                               source_device_ptr + source_pitch * height,
                               source_pitch, uv_width_bytes, uv_height);
  }
  PopContext();
  if (!ok) {
    return false;
  }

  const int result = libyuv::NV12ToI420(
      y.data(), width, uv.data(), uv_width_bytes, destination->MutableDataY(),
      destination->StrideY(), destination->MutableDataU(),
      destination->StrideU(), destination->MutableDataV(),
      destination->StrideV(), width, height);
  if (result != 0) {
    RTC_LOG(LS_WARNING) << "libyuv::NV12ToI420 failed: " << result;
    return false;
  }
  return true;
}

bool CopyNvidiaCudaNv12ToExternalImages(CUcontext context,
                                        CUdeviceptr source_device_ptr,
                                        uint32_t source_pitch,
                                        uint32_t width,
                                        uint32_t height,
                                        int32_t y_fd,
                                        uint64_t y_allocation_size,
                                        int32_t uv_fd,
                                        uint64_t uv_allocation_size) {
  if (!PushContext(context)) {
    return false;
  }

  const uint32_t uv_width = (width + 1) / 2;
  const uint32_t uv_height = ChromaHeight(height);
  bool ok = CopyPlaneToExternalImage(source_device_ptr, source_pitch, width,
                                     height, 1, width, y_fd,
                                     y_allocation_size);
  if (ok) {
    ok = CopyPlaneToExternalImage(
        source_device_ptr + source_pitch * height, source_pitch, uv_width,
        uv_height, 2, ChromaWidthBytes(width), uv_fd, uv_allocation_size);
  }

  PopContext();
  return ok;
}

}  // namespace webrtc
