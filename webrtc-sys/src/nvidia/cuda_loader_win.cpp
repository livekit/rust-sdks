// The CUDA driver and NVDEC calls the NVIDIA encoder and decoder make, resolved from
// nvcuda.dll and nvcuvid.dll at run time (Linux does the same with lazy-load stubs). Linking
// cuda.lib or nvcuvid.lib instead would stop an app from starting on PCs without an NVIDIA
// driver.
#include <cuda.h>
#include <windows.h>

#include "nvcuvid.h"

namespace {

FARPROC Resolve(const char* name) {
  static HMODULE module =
      LoadLibraryExA("nvcuda.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
  return module ? GetProcAddress(module, name) : nullptr;
}

FARPROC ResolveCuvid(const char* name) {
  static HMODULE module =
      LoadLibraryExA("nvcuvid.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
  return module ? GetProcAddress(module, name) : nullptr;
}

}  // namespace

// Stringized after expansion, so cuCtxCreate resolves cuCtxCreate_v2 as cuda.h declares it.
#define CU_NAME(name) #name
#define CU_FORWARD(name, params, args)                                \
  extern "C" CUresult CUDAAPI name params {                           \
    static auto fn = reinterpret_cast<decltype(&name)>(Resolve(CU_NAME(name))); \
    return fn ? fn args : CUDA_ERROR_NOT_INITIALIZED;                 \
  }
#define CU_EXPAND(name, params, args) CU_FORWARD(name, params, args)
#define CUVID_FORWARD(name, params, args)                                  \
  extern "C" CUresult CUDAAPI name params {                                \
    static auto fn = reinterpret_cast<decltype(&name)>(ResolveCuvid(CU_NAME(name))); \
    return fn ? fn args : CUDA_ERROR_NOT_INITIALIZED;                      \
  }
#define CUVID_EXPAND(name, params, args) CUVID_FORWARD(name, params, args)

CU_EXPAND(cuInit, (unsigned int flags), (flags))
CU_EXPAND(cuDriverGetVersion, (int* version), (version))
CU_EXPAND(cuDeviceGet, (CUdevice* device, int ordinal), (device, ordinal))
CU_EXPAND(cuDeviceGetCount, (int* count), (count))
CU_EXPAND(cuDeviceGetName, (char* name, int len, CUdevice dev), (name, len, dev))
CU_EXPAND(cuGetErrorName, (CUresult error, const char** str), (error, str))
#if CUDA_VERSION >= 13000
// Drivers older than CUDA 13 lack cuCtxCreate_v4; without extended parameters the
// cuCtxCreate_v2 they export does the same.
extern "C" CUresult CUDAAPI cuCtxCreate(CUcontext* ctx, CUctxCreateParams* params,
                                        unsigned int flags, CUdevice dev) {
  // Named directly: CU_NAME(cuCtxCreate) would stringize the unexpanded macro name.
  static auto fn = reinterpret_cast<decltype(&cuCtxCreate)>(Resolve("cuCtxCreate_v4"));
  using Legacy = CUresult(CUDAAPI*)(CUcontext*, unsigned int, CUdevice);
  static auto legacy = reinterpret_cast<Legacy>(Resolve("cuCtxCreate_v2"));
  if (fn) {
    return fn(ctx, params, flags, dev);
  }
  return legacy && !params ? legacy(ctx, flags, dev) : CUDA_ERROR_NOT_INITIALIZED;
}
#else
CU_EXPAND(cuCtxCreate, (CUcontext* ctx, unsigned int flags, CUdevice dev), (ctx, flags, dev))
#endif
CU_EXPAND(cuCtxDestroy, (CUcontext ctx), (ctx))
CU_EXPAND(cuCtxGetCurrent, (CUcontext* ctx), (ctx))
CU_EXPAND(cuCtxGetDevice, (CUdevice* device), (device))
CU_EXPAND(cuDeviceGetAttribute, (int* value, CUdevice_attribute attrib, CUdevice dev),
          (value, attrib, dev))
CU_EXPAND(cuCtxSetCurrent, (CUcontext ctx), (ctx))
CU_EXPAND(cuCtxPushCurrent, (CUcontext ctx), (ctx))
CU_EXPAND(cuCtxPopCurrent, (CUcontext* ctx), (ctx))
CU_EXPAND(cuMemAllocPitch,
          (CUdeviceptr* ptr, size_t* pitch, size_t width, size_t height, unsigned int element_size),
          (ptr, pitch, width, height, element_size))
CU_EXPAND(cuMemAlloc, (CUdeviceptr* ptr, size_t size), (ptr, size))
CU_EXPAND(cuMemFree, (CUdeviceptr ptr), (ptr))
CU_EXPAND(cuMemcpy2D, (const CUDA_MEMCPY2D* copy), (copy))
CU_EXPAND(cuMemcpy2DUnaligned, (const CUDA_MEMCPY2D* copy), (copy))
CU_EXPAND(cuMemcpy2DAsync, (const CUDA_MEMCPY2D* copy, CUstream stream), (copy, stream))
CU_EXPAND(cuArrayDestroy, (CUarray array), (array))
CU_EXPAND(cuStreamCreate, (CUstream* stream, unsigned int flags), (stream, flags))
CU_EXPAND(cuStreamSynchronize, (CUstream stream), (stream))
CU_EXPAND(cuStreamDestroy, (CUstream stream), (stream))

CUVID_EXPAND(cuvidCtxLockCreate, (CUvideoctxlock* lock, CUcontext ctx), (lock, ctx))
CUVID_EXPAND(cuvidCtxLockDestroy, (CUvideoctxlock lock), (lock))
CUVID_EXPAND(cuvidGetDecoderCaps, (CUVIDDECODECAPS* caps), (caps))
CUVID_EXPAND(cuvidCreateDecoder, (CUvideodecoder* decoder, CUVIDDECODECREATEINFO* info),
             (decoder, info))
CUVID_EXPAND(cuvidDestroyDecoder, (CUvideodecoder decoder), (decoder))
CUVID_EXPAND(cuvidReconfigureDecoder,
             (CUvideodecoder decoder, CUVIDRECONFIGUREDECODERINFO* params), (decoder, params))
CUVID_EXPAND(cuvidDecodePicture, (CUvideodecoder decoder, CUVIDPICPARAMS* params),
             (decoder, params))
CUVID_EXPAND(cuvidGetDecodeStatus,
             (CUvideodecoder decoder, int index, CUVIDGETDECODESTATUS* status),
             (decoder, index, status))
CUVID_EXPAND(cuvidMapVideoFrame,
             (CUvideodecoder decoder, int index, unsigned long long* ptr, unsigned int* pitch,
              CUVIDPROCPARAMS* params),
             (decoder, index, ptr, pitch, params))
CUVID_EXPAND(cuvidUnmapVideoFrame, (CUvideodecoder decoder, unsigned long long ptr),
             (decoder, ptr))
CUVID_EXPAND(cuvidCreateVideoParser, (CUvideoparser* parser, CUVIDPARSERPARAMS* params),
             (parser, params))
CUVID_EXPAND(cuvidParseVideoData, (CUvideoparser parser, CUVIDSOURCEDATAPACKET* packet),
             (parser, packet))
CUVID_EXPAND(cuvidDestroyVideoParser, (CUvideoparser parser), (parser))
