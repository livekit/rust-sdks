#include "vaapi_encoder_factory.h"

#include <memory>

#include "h264_encoder_impl.h"

#include <iostream>

#if defined(WIN32)
#include "vaapi_display_win32.h"
using VaapiDisplay = livekit::VaapiDisplayWin32;
#elif defined(__linux__)
#include "vaapi_display_drm.h"
using VaapiDisplay = livekit::VaapiDisplayDrm ;
#endif

namespace webrtc {

VAAPIVideoEncoderFactory::VAAPIVideoEncoderFactory() {
  std::map<std::string, std::string> baselineParameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));
  /*
  std::map<std::string, std::string> highParameters = {
      {"profile-level-id", "4d0032"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };

  supported_formats_.push_back(SdpVideoFormat("H264", highParameters));
  */
}

VAAPIVideoEncoderFactory::~VAAPIVideoEncoderFactory() {}

bool VAAPIVideoEncoderFactory::IsSupported() {
  // Check if VAAPI is supported by the environment.
  // This could involve checking if the VAAPI display can be opened.
  VaapiDisplay vaapi_display;
  if (!vaapi_display.Open()) {
    std::cerr << "Failed to open VAAPI display." << std::endl;
    return false;
  }

  vaapi_display.Close();
  // If we can open the VAAPI display, we consider it supported.
  std::cout << "VAAPI is supported." << std::endl;
  return true;
}

std::unique_ptr<VideoEncoder> VAAPIVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      // If the format is supported, create and return the encoder.
      return std::make_unique<VAAPIH264EncoderWrapper>(env, format);
    }
  }
  return nullptr;
}
std::vector<SdpVideoFormat> VAAPIVideoEncoderFactory::GetSupportedFormats()
    const {
  return supported_formats_;
}

std::vector<SdpVideoFormat> VAAPIVideoEncoderFactory::GetImplementations()
    const {
  return supported_formats_;
}

}  // namespace webrtc
