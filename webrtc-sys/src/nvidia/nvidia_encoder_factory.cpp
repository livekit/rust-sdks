#include "nvidia_encoder_factory.h"

#include <memory>

#include "cuda_context.h"
#include "h264_encoder_impl.h"
#include "rtc_base/logging.h"

namespace webrtc {

NvidiaVideoEncoderFactory::NvidiaVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));

  /*std::map<std::string, std::string> highParameters = {
      {"profile-level-id", "4d0032"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };

  supported_formats_.push_back(SdpVideoFormat("H264", highParameters));
  */
}

NvidiaVideoEncoderFactory::~NvidiaVideoEncoderFactory() {}

bool NvidiaVideoEncoderFactory::IsSupported() {
  if (!livekit::CudaContext::IsAvailable()) {
    RTC_LOG(LS_WARNING) << "Cuda Context is not available.";
    return false;
  }

  std::cout << "Nvidia Encoder is supported." << std::endl;
  return true;
}

std::unique_ptr<VideoEncoder> NvidiaVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      // If the format is supported, create and return the encoder.
      if (!cu_context_) {
        cu_context_ = livekit::CudaContext::GetInstance();
        if (!cu_context_->Initialize()) {
          RTC_LOG(LS_ERROR) << "Failed to initialize CUDA context.";
          return nullptr;
        }
      }
      return std::make_unique<NvidiaH264EncoderImpl>(
          env, cu_context_->GetContext(), CU_MEMORYTYPE_DEVICE,
          NV_ENC_BUFFER_FORMAT_IYUV, format);
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
