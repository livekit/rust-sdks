#ifndef WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H
#define WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H

#include <cuda.h>

namespace livekit {

class CudaContext {
 public:
  CudaContext() = default;
  ~CudaContext() { Shutdown(); }

  bool Initialize();
  bool IsInitialized() const { return cu_context_ != nullptr; }
  CUcontext GetContext() const { return cu_context_; }
  CUdevice GetDevice() const { return cu_device_; }
  void Shutdown();

 private:
  CUdevice cu_device_ = 0;
  CUcontext cu_context_ = nullptr;
};

}  // namespace livekit

#endif  // WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H
