#include "nvidia_encoder_factory.h"

#include <algorithm>
#include <cstring>
#include <map>
#include <memory>
#include <string>
#include <vector>

#include "absl/container/inlined_vector.h"
#include "api/video_codecs/scalability_mode.h"
#include "av1_encoder_impl.h"
#include "cuda_context.h"
#include "h264_encoder_impl.h"
#include "h265_encoder_impl.h"
#include "nvEncodeAPI.h"
#include "rtc_base/logging.h"

#include <dlfcn.h>

namespace webrtc {

namespace {

struct NvencProbeResult {
  bool encoder_supported = false;
  bool av1_supported = false;
};

bool GuidEquals(const GUID& lhs, const GUID& rhs) {
  return std::memcmp(&lhs, &rhs, sizeof(GUID)) == 0;
}

bool SupportsEncodeGuid(NV_ENCODE_API_FUNCTION_LIST& fnList,
                        void* hEncoder,
                        const GUID& encodeGuid) {
  uint32_t guid_count = 0;
  NVENCSTATUS nvStatus =
      fnList.nvEncGetEncodeGUIDCount(hEncoder, &guid_count);
  if (nvStatus != NV_ENC_SUCCESS || guid_count == 0) {
    return false;
  }

  std::vector<GUID> guids(guid_count);
  uint32_t written_guid_count = 0;
  nvStatus = fnList.nvEncGetEncodeGUIDs(hEncoder, guids.data(), guid_count,
                                        &written_guid_count);
  if (nvStatus != NV_ENC_SUCCESS) {
    return false;
  }

  written_guid_count = std::min(written_guid_count, guid_count);
  return std::any_of(guids.begin(), guids.begin() + written_guid_count,
                     [&](const GUID& guid) {
                       return GuidEquals(guid, encodeGuid);
                     });
}

bool SupportsInputFormat(NV_ENCODE_API_FUNCTION_LIST& fnList,
                         void* hEncoder,
                         const GUID& encodeGuid,
                         NV_ENC_BUFFER_FORMAT required_format) {
  uint32_t format_count = 0;
  NVENCSTATUS nvStatus =
      fnList.nvEncGetInputFormatCount(hEncoder, encodeGuid, &format_count);
  if (nvStatus != NV_ENC_SUCCESS || format_count == 0) {
    return false;
  }

  std::vector<NV_ENC_BUFFER_FORMAT> formats(format_count);
  uint32_t written_format_count = 0;
  nvStatus = fnList.nvEncGetInputFormats(
      hEncoder, encodeGuid, formats.data(), format_count, &written_format_count);
  if (nvStatus != NV_ENC_SUCCESS) {
    return false;
  }

  written_format_count = std::min(written_format_count, format_count);
  return std::any_of(formats.begin(), formats.begin() + written_format_count,
                     [&](NV_ENC_BUFFER_FORMAT format) {
                       return format == required_format;
                     });
}

bool SupportsPositiveDimensionCaps(NV_ENCODE_API_FUNCTION_LIST& fnList,
                                   void* hEncoder,
                                   const GUID& encodeGuid) {
  NV_ENC_CAPS_PARAM capsParam = {NV_ENC_CAPS_PARAM_VER};

  int max_width = 0;
  capsParam.capsToQuery = NV_ENC_CAPS_WIDTH_MAX;
  if (fnList.nvEncGetEncodeCaps(hEncoder, encodeGuid, &capsParam,
                                &max_width) != NV_ENC_SUCCESS ||
      max_width <= 0) {
    return false;
  }

  int max_height = 0;
  capsParam.capsToQuery = NV_ENC_CAPS_HEIGHT_MAX;
  if (fnList.nvEncGetEncodeCaps(hEncoder, encodeGuid, &capsParam,
                                &max_height) != NV_ENC_SUCCESS ||
      max_height <= 0) {
    return false;
  }

  return true;
}

bool SupportsAv1Encoding(NV_ENCODE_API_FUNCTION_LIST& fnList,
                         void* hEncoder) {
  return SupportsEncodeGuid(fnList, hEncoder, NV_ENC_CODEC_AV1_GUID) &&
         SupportsInputFormat(fnList, hEncoder, NV_ENC_CODEC_AV1_GUID,
                             NV_ENC_BUFFER_FORMAT_IYUV) &&
         SupportsPositiveDimensionCaps(fnList, hEncoder,
                                       NV_ENC_CODEC_AV1_GUID);
}

NvencProbeResult ProbeNvencSupport() {
  NvencProbeResult result;
  if (!livekit_ffi::CudaContext::IsAvailable()) {
    RTC_LOG(LS_WARNING) << "CUDA is not available, NVENC disabled.";
    return result;
  }

  // CUDA being available does NOT imply NVENC is present. Compute-only GPUs
  // (H100, A100, etc.) have full CUDA support but no encode hardware.
  // Probe the NVENC library and try to open a session to be sure.
  void* hModule = dlopen("libnvidia-encode.so.1", RTLD_LAZY);
  if (!hModule) {
    RTC_LOG(LS_WARNING) << "NVENC library (libnvidia-encode.so.1) not found, "
                           "hardware encoding unavailable.";
    return result;
  }

  auto NvEncodeAPIGetMaxSupportedVersion =
      (NVENCSTATUS(NVENCAPI*)(uint32_t*))dlsym(
          hModule, "NvEncodeAPIGetMaxSupportedVersion");
  auto NvEncodeAPICreateInstance =
      (NVENCSTATUS(NVENCAPI*)(NV_ENCODE_API_FUNCTION_LIST*))dlsym(
          hModule, "NvEncodeAPICreateInstance");

  NV_ENCODE_API_FUNCTION_LIST fnList = {NV_ENCODE_API_FUNCTION_LIST_VER};
  CUcontext cuCtx = nullptr;
  void* hEncoder = nullptr;

  do {
    if (!NvEncodeAPIGetMaxSupportedVersion || !NvEncodeAPICreateInstance) {
      RTC_LOG(LS_WARNING) << "NVENC API entry points not found in library.";
      break;
    }

    uint32_t maxVersion = 0;
    if (NvEncodeAPIGetMaxSupportedVersion(&maxVersion) != NV_ENC_SUCCESS) {
      RTC_LOG(LS_WARNING) << "NvEncodeAPIGetMaxSupportedVersion failed.";
      break;
    }

    uint32_t currentVersion =
        (NVENCAPI_MAJOR_VERSION << 4) | NVENCAPI_MINOR_VERSION;
    if (currentVersion > maxVersion) {
      RTC_LOG(LS_WARNING) << "NVENC driver version too old: driver supports "
                          << maxVersion << ", SDK requires " << currentVersion;
      break;
    }

    if (NvEncodeAPICreateInstance(&fnList) != NV_ENC_SUCCESS) {
      RTC_LOG(LS_WARNING) << "NvEncodeAPICreateInstance failed.";
      break;
    }

    CUresult cuRes = cuInit(0);
    if (cuRes != CUDA_SUCCESS) {
      RTC_LOG(LS_WARNING) << "cuInit failed during NVENC probe.";
      break;
    }

    CUdevice cuDevice;
    cuRes = cuDeviceGet(&cuDevice, 0);
    if (cuRes != CUDA_SUCCESS) {
      RTC_LOG(LS_WARNING) << "cuDeviceGet failed during NVENC probe.";
      break;
    }

#if CUDA_VERSION >= 13000
    cuRes = cuCtxCreate(&cuCtx, nullptr, 0, cuDevice);
#else
    cuRes = cuCtxCreate(&cuCtx, 0, cuDevice);
#endif
    if (cuRes != CUDA_SUCCESS || !cuCtx) {
      RTC_LOG(LS_WARNING) << "cuCtxCreate failed during NVENC probe.";
      break;
    }

    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS sessionParams = {
        NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER};
    sessionParams.device = cuCtx;
    sessionParams.deviceType = NV_ENC_DEVICE_TYPE_CUDA;
    sessionParams.apiVersion = NVENCAPI_VERSION;

    NVENCSTATUS nvStatus =
        fnList.nvEncOpenEncodeSessionEx(&sessionParams, &hEncoder);
    if (nvStatus != NV_ENC_SUCCESS || !hEncoder) {
      char deviceName[80] = {};
      cuDeviceGetName(deviceName, sizeof(deviceName), cuDevice);
      RTC_LOG(LS_WARNING) << "NVENC not available on GPU \"" << deviceName
                          << "\" (status=" << nvStatus
                          << "). This GPU likely lacks encode hardware.";
      break;
    }

    result.encoder_supported = true;
    result.av1_supported = SupportsAv1Encoding(fnList, hEncoder);
  } while (false);

  if (hEncoder) {
    fnList.nvEncDestroyEncoder(hEncoder);
  }
  if (cuCtx) {
    cuCtxDestroy(cuCtx);
  }
  dlclose(hModule);

  return result;
}

}  // namespace

NvidiaVideoEncoderFactory::NvidiaVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  // Advertise HEVC/H265 with default parameters.
  supported_formats_.push_back(SdpVideoFormat("H265"));
  // Some stacks use 'HEVC' name.
  supported_formats_.push_back(SdpVideoFormat("HEVC"));

  if (IsAv1Supported()) {
    absl::InlinedVector<ScalabilityMode, kScalabilityModeCount>
        scalability_modes;
    scalability_modes.push_back(ScalabilityMode::kL1T1);
    supported_formats_.push_back(
        SdpVideoFormat(SdpVideoFormat::AV1Profile0(), scalability_modes));
    RTC_LOG(LS_INFO) << "NVIDIA AV1 NVENC encoder is available.";
  } else {
    RTC_LOG(LS_INFO)
        << "NVIDIA AV1 NVENC encoder is not supported on this GPU.";
  }
}

NvidiaVideoEncoderFactory::~NvidiaVideoEncoderFactory() {}

namespace {

const NvencProbeResult& CachedNvencProbe() {
  static const NvencProbeResult probe = [] {
    NvencProbeResult result = ProbeNvencSupport();
    RTC_LOG(LS_INFO) << "NVIDIA NVENC hardware encoder "
                     << (result.encoder_supported ? "is available."
                                                  : "is not available.");
    return result;
  }();
  return probe;
}

}  // namespace

bool NvidiaVideoEncoderFactory::IsSupported() {
  return CachedNvencProbe().encoder_supported;
}

bool NvidiaVideoEncoderFactory::IsAv1Supported() {
  return CachedNvencProbe().av1_supported;
}

std::unique_ptr<VideoEncoder> NvidiaVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (!cu_context_) {
        cu_context_ = livekit_ffi::CudaContext::GetInstance();
        if (!cu_context_->Initialize()) {
          RTC_LOG(LS_ERROR) << "Failed to initialize CUDA context.";
          return nullptr;
        }
      }

      if (format.name == "H264") {
        RTC_LOG(LS_INFO) << "Using NVIDIA HW encoder (NVENC) for H264";
        return std::make_unique<NvidiaH264EncoderImpl>(
            env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
            NV_ENC_BUFFER_FORMAT_IYUV, format);
      }

      if (format.name == "H265" || format.name == "HEVC") {
        RTC_LOG(LS_INFO) << "Using NVIDIA HW encoder (NVENC) for H265/HEVC";
        return std::make_unique<NvidiaH265EncoderImpl>(
            env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
            NV_ENC_BUFFER_FORMAT_IYUV, format);
      }

      if (format.name == "AV1") {
        RTC_LOG(LS_INFO) << "Using NVIDIA HW encoder (NVENC) for AV1";
        return std::make_unique<NvidiaAV1EncoderImpl>(
            env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
            NV_ENC_BUFFER_FORMAT_IYUV, format);
      }
    }
  }
  return nullptr;
}
std::vector<SdpVideoFormat> NvidiaVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> NvidiaVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
