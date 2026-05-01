#include "vaapi_encoder_factory.h"

#include <memory>
#include <iostream>
#include <dlfcn.h>
#include <map>

#include "h264_encoder_impl.h"
#include "h265_encoder_impl.h"
#include "rtc_base/logging.h"

#if defined(WIN32)
#include "vaapi_display_win32.h"
using VaapiDisplay = livekit_ffi::VaapiDisplayWin32;
#elif defined(__linux__)
#include "vaapi_display_drm.h"
using VaapiDisplay = livekit_ffi::VaapiDisplayDrm;
#endif

namespace webrtc {

VAAPIVideoEncoderFactory::VAAPIVideoEncoderFactory() {
  if (IsH264Supported()) {
    std::map<std::string, std::string> baselineParameters = {
        {"profile-level-id", "42e01f"},
        {"level-asymmetry-allowed", "1"},
        {"packetization-mode", "1"},
    };
    supported_formats_.push_back(SdpVideoFormat("H264", baselineParameters));
  }

  if (IsH265Supported()) {
    supported_formats_.push_back(SdpVideoFormat("H265"));
    supported_formats_.push_back(SdpVideoFormat("HEVC"));
  }
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
  return IsH264Supported() || IsH265Supported();
}

bool VAAPIVideoEncoderFactory::CanLoadLibva() {
  // Ensure that libva and libva-drm are actually available for loading.
  // Otherwise, we will immediately abort.
  void* libva_ptr = dlopen("libva.so.2", RTLD_LAZY);
  if (!libva_ptr) {
    RTC_LOG(LS_INFO) << "libva.so.2 is not found";
    return false;
  }
  dlclose(libva_ptr);

  void* libvadrm_ptr = dlopen("libva-drm.so.2", RTLD_LAZY);
  if (!libvadrm_ptr) {
    RTC_LOG(LS_INFO) << "libva-drm.so.2 is not found";
    return false;
  }
  dlclose(libvadrm_ptr);

  return true;
}

bool VAAPIVideoEncoderFactory::IsH264Supported() {
  if (!CanLoadLibva()) {
    return false;
  }

  // Check if VAAPI is supported by the environment.
  // This could involve checking if the VAAPI display can be opened.
  VaapiDisplay vaapi_display;
  if (!vaapi_display.Open()) {
    RTC_LOG(LS_WARNING) << "Failed to open VAAPI display.";
    return false;
  }

  bool supported = vaapi_display.SupportsH264Encode();
  vaapi_display.Close();
  if (supported) {
    RTC_LOG(LS_INFO) << "VAAPI H264 encoder is supported.";
  }
  return supported;
}

bool VAAPIVideoEncoderFactory::IsH265Supported() {
  if (!CanLoadLibva()) {
    return false;
  }

  VaapiDisplay vaapi_display;
  if (!vaapi_display.Open()) {
    RTC_LOG(LS_WARNING) << "Failed to open VAAPI display.";
    return false;
  }

  bool supported = vaapi_display.SupportsH265Encode();
  vaapi_display.Close();
  if (supported) {
    RTC_LOG(LS_INFO) << "VAAPI H265 encoder is supported.";
  }
  return supported;
}

std::unique_ptr<VideoEncoder> VAAPIVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Check if the requested format is supported.
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      if (format.name == "H264") {
        return std::make_unique<VAAPIH264EncoderWrapper>(env, format);
      }

      if (format.name == "H265" || format.name == "HEVC") {
        return std::make_unique<VAAPIH265EncoderWrapper>(env, format);
      }
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
