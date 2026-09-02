#ifndef WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H
#define WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H

#include <cuda.h>

namespace livekit_ffi {

/// @brief Process-wide CUDA context shared by NVIDIA codec factories.
///
/// Every successful Initialize() call acquires one reference and must be
/// paired with one Shutdown() call. The underlying CUDA context is destroyed
/// when the final reference is released.
class CudaContext {
 public:
  CudaContext() = default;
  ~CudaContext() = default;

  /// @brief Checks whether a compatible CUDA device and driver are available.
  /// @return True when CUDA can be used.
  static bool IsAvailable();

  /// @brief Returns the process-wide context manager.
  /// @return Non-owning pointer to the singleton context manager.
  static CudaContext* GetInstance();

  /// @brief Acquires a reference to the CUDA context, creating it if needed.
  /// @return True on success; false if CUDA initialization fails.
  bool Initialize();

  /// @brief Reports whether the underlying CUDA context exists.
  /// @return True after successful initialization and before final shutdown.
  bool IsInitialized() const { return cu_context_ != nullptr; }

  /// @brief Returns the CUDA context and makes it current on the calling
  /// thread.
  /// @return The initialized CUDA context.
  CUcontext GetContext() const;

  /// @brief Releases one reference and destroys the context after the last one.
  void Shutdown();

 private:
  CUdevice cu_device_ = 0;
  CUcontext cu_context_ = nullptr;
  // Guarded by cudaMutex() in cuda_context.cpp.
  int ref_count_ = 0;
};

}  // namespace livekit_ffi

#endif  // WEBRTC_SYS_NVIDIA_CUDA_CONTEXT_H
