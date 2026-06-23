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

#include "livekit/video_encoder_factory.h"

#include <cstdlib>
#include <optional>
#include <string_view>
#include <utility>

#include "api/environment/environment_factory.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory_template.h"
#include "livekit/objc_video_factory.h"
#include "livekit/passthrough_video_encoder.h"
#include "livekit/webrtc.h"
#include "media/base/media_constants.h"
#include "media/engine/simulcast_encoder_adapter.h"
#include "rtc_base/logging.h"
#include "rust/cxx.h"
#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
#include "api/video_codecs/video_encoder_factory_template_libaom_av1_adapter.h"
#endif
#if defined(WEBRTC_USE_H264)
#include "api/video_codecs/video_encoder_factory_template_open_h264_adapter.h"
#endif
#include "api/video_codecs/video_encoder_factory_template_libvpx_vp8_adapter.h"
#include "api/video_codecs/video_encoder_factory_template_libvpx_vp9_adapter.h"

#ifdef WEBRTC_ANDROID
#include "livekit/android.h"
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
#include "nvidia/nvidia_encoder_factory.h"
#endif

#if defined(USE_VAAPI_VIDEO_CODEC)
#include "vaapi/vaapi_encoder_factory.h"
#endif

#if defined(USE_JETSON_VIDEO_CODEC)
#include "jetson/jetson_encoder_factory.h"
#endif

namespace livekit_ffi {

namespace {

constexpr char kBackendParameter[] = "x-livekit-video-encoder-backend";
constexpr char kPreferredHwEncoderEnv[] = "LIVEKIT_PREFERRED_HW_ENCODER";

enum class PreferredHwEncoder {
  kNvenc,
  kVaapi,
};

struct PreferredHwEncoderConfig {
  PreferredHwEncoder encoder = PreferredHwEncoder::kNvenc;
  bool explicitly_set = false;
};

PreferredHwEncoderConfig GetPreferredHwEncoderConfig() {
  const char* preferred_encoder = std::getenv(kPreferredHwEncoderEnv);
  if (!preferred_encoder) {
    return {};
  }

  std::string_view preferred_encoder_view(preferred_encoder);
  if (preferred_encoder_view == "nvenc") {
    return {PreferredHwEncoder::kNvenc, true};
  }
  if (preferred_encoder_view == "vaapi") {
    return {PreferredHwEncoder::kVaapi, true};
  }

  RTC_LOG(LS_WARNING) << "Ignoring invalid LIVEKIT_PREFERRED_HW_ENCODER=\""
                      << preferred_encoder
                      << "\"; expected \"nvenc\" or \"vaapi\".";
  return {};
}

const char* BackendName(VideoEncoderBackend backend) {
  switch (backend) {
    case VideoEncoderBackend::Auto:
      return "auto";
    case VideoEncoderBackend::Software:
      return "software";
    case VideoEncoderBackend::Hardware:
      return "hardware";
    case VideoEncoderBackend::Nvenc:
      return "nvenc";
    case VideoEncoderBackend::Vaapi:
      return "vaapi";
    case VideoEncoderBackend::VideoToolbox:
      return "videotoolbox";
    case VideoEncoderBackend::PreEncoded:
      return "preencoded";
  }
}

std::optional<VideoEncoderBackend> BackendFromFormat(
    const webrtc::SdpVideoFormat& format) {
  auto it = format.parameters.find(kBackendParameter);
  if (it == format.parameters.end()) {
    return std::nullopt;
  }

  if (it->second == BackendName(VideoEncoderBackend::Software)) {
    return VideoEncoderBackend::Software;
  }
  if (it->second == BackendName(VideoEncoderBackend::Hardware)) {
    return VideoEncoderBackend::Hardware;
  }
  if (it->second == BackendName(VideoEncoderBackend::Nvenc)) {
    return VideoEncoderBackend::Nvenc;
  }
  if (it->second == BackendName(VideoEncoderBackend::Vaapi)) {
    return VideoEncoderBackend::Vaapi;
  }
  if (it->second == BackendName(VideoEncoderBackend::VideoToolbox)) {
    return VideoEncoderBackend::VideoToolbox;
  }
  if (it->second == BackendName(VideoEncoderBackend::PreEncoded)) {
    return VideoEncoderBackend::PreEncoded;
  }

  return std::nullopt;
}

webrtc::SdpVideoFormat StripBackendParameter(
    const webrtc::SdpVideoFormat& format) {
  webrtc::SdpVideoFormat stripped = format;
  stripped.parameters.erase(kBackendParameter);
  return stripped;
}

webrtc::SdpVideoFormat WithBackend(
    const webrtc::SdpVideoFormat& format,
    VideoEncoderBackend backend) {
  webrtc::SdpVideoFormat tagged = format;
  tagged.parameters[kBackendParameter] = BackendName(backend);
  return tagged;
}

bool IsSpecificHardwareBackend(VideoEncoderBackend backend) {
  return backend == VideoEncoderBackend::Nvenc ||
         backend == VideoEncoderBackend::Vaapi ||
         backend == VideoEncoderBackend::VideoToolbox;
}

bool BackendMatches(VideoEncoderBackend requested, VideoEncoderBackend actual) {
  return requested == actual ||
         (requested == VideoEncoderBackend::Hardware &&
          actual != VideoEncoderBackend::Software &&
          actual != VideoEncoderBackend::Auto);
}

void AddBackendFactory(
    std::vector<VideoEncoderBackendFactory>& factories,
    VideoEncoderBackend backend,
    std::unique_ptr<webrtc::VideoEncoderFactory> factory) {
  factories.push_back(VideoEncoderBackendFactory{backend, std::move(factory)});
}

void AddJetsonFactory(
    std::vector<VideoEncoderBackendFactory>& factories) {
#if defined(USE_JETSON_VIDEO_CODEC)
  if (webrtc::JetsonVideoEncoderFactory::IsSupported()) {
    AddBackendFactory(
        factories,
        VideoEncoderBackend::Hardware,
        std::make_unique<webrtc::JetsonVideoEncoderFactory>());
    return;
  }
#else
  (void)factories;
#endif
}

void AddNvencFactory(
    std::vector<VideoEncoderBackendFactory>& factories,
    bool preferred) {
#if defined(USE_NVIDIA_VIDEO_CODEC)
  if (webrtc::NvidiaVideoEncoderFactory::IsSupported()) {
    AddBackendFactory(
        factories,
        VideoEncoderBackend::Nvenc,
        std::make_unique<webrtc::NvidiaVideoEncoderFactory>());
    return;
  }

  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=nvenc requested, but NVENC "
           "is unavailable; falling back to other encoders.";
  }
#else
  (void)factories;
  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=nvenc requested, but NVENC support "
           "is not compiled in; falling back to other encoders.";
  }
#endif
}

void AddVaapiFactory(
    std::vector<VideoEncoderBackendFactory>& factories,
    bool preferred) {
#if defined(USE_VAAPI_VIDEO_CODEC)
  if (webrtc::VAAPIVideoEncoderFactory::IsSupported()) {
    AddBackendFactory(
        factories,
        VideoEncoderBackend::Vaapi,
        std::make_unique<webrtc::VAAPIVideoEncoderFactory>());
    return;
  }

  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=vaapi requested, but VAAPI "
           "is unavailable; falling back to other encoders.";
  }
#else
  (void)factories;
  if (preferred) {
    RTC_LOG(LS_WARNING)
        << "LIVEKIT_PREFERRED_HW_ENCODER=vaapi requested, but VAAPI support "
           "is not compiled in; falling back to other encoders.";
  }
#endif
}

}  // namespace

using Factory = webrtc::VideoEncoderFactoryTemplate<
    webrtc::LibvpxVp8EncoderTemplateAdapter,
#if defined(WEBRTC_USE_H264)
    webrtc::OpenH264EncoderTemplateAdapter,
#endif
#if defined(RTC_USE_LIBAOM_AV1_ENCODER)
    webrtc::LibaomAv1EncoderTemplateAdapter,
#endif
    webrtc::LibvpxVp9EncoderTemplateAdapter>;

rust::Vec<VideoEncoderBackend> video_encoder_backend_list() {
  rust::Vec<VideoEncoderBackend> backends;
  backends.push_back(VideoEncoderBackend::Auto);
  backends.push_back(VideoEncoderBackend::Software);
  backends.push_back(VideoEncoderBackend::PreEncoded);

  bool has_hardware_backend = false;
  bool hardware_backend_listed = false;

#ifdef __APPLE__
  backends.push_back(VideoEncoderBackend::VideoToolbox);
  has_hardware_backend = true;
#endif

#ifdef WEBRTC_ANDROID
  backends.push_back(VideoEncoderBackend::Hardware);
  has_hardware_backend = true;
  hardware_backend_listed = true;
#endif

#if defined(USE_JETSON_VIDEO_CODEC)
  if (webrtc::JetsonVideoEncoderFactory::IsSupported()) {
    has_hardware_backend = true;
  }
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
  if (webrtc::NvidiaVideoEncoderFactory::IsSupported()) {
    backends.push_back(VideoEncoderBackend::Nvenc);
    has_hardware_backend = true;
  }
#endif

#if defined(USE_VAAPI_VIDEO_CODEC)
  if (webrtc::VAAPIVideoEncoderFactory::IsSupported()) {
    backends.push_back(VideoEncoderBackend::Vaapi);
    has_hardware_backend = true;
  }
#endif

  if (has_hardware_backend && !hardware_backend_listed) {
    backends.push_back(VideoEncoderBackend::Hardware);
  }

  return backends;
}

VideoEncoderFactory::InternalFactory::InternalFactory() {
  AddBackendFactory(
      factories_,
      VideoEncoderBackend::PreEncoded,
      std::make_unique<livekit_ffi::PassthroughVideoEncoderFactory>());

#ifdef __APPLE__
  AddBackendFactory(
      factories_,
      VideoEncoderBackend::VideoToolbox,
      livekit_ffi::CreateObjCVideoEncoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  AddBackendFactory(
      factories_,
      VideoEncoderBackend::Hardware,
      CreateAndroidVideoEncoderFactory());
#endif

  AddJetsonFactory(factories_);

  const PreferredHwEncoderConfig preferred_hw_encoder =
      GetPreferredHwEncoderConfig();
  if (preferred_hw_encoder.encoder == PreferredHwEncoder::kVaapi) {
    AddVaapiFactory(factories_, preferred_hw_encoder.explicitly_set);
    AddNvencFactory(factories_, false);
  } else {
    AddNvencFactory(factories_, preferred_hw_encoder.explicitly_set);
    AddVaapiFactory(factories_, false);
  }
}

std::vector<webrtc::SdpVideoFormat>
VideoEncoderFactory::InternalFactory::GetSupportedFormats() const {
  std::vector<webrtc::SdpVideoFormat> formats = Factory().GetSupportedFormats();

  for (const auto& backend_factory : factories_) {
    auto supported_formats = backend_factory.factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }
  return formats;
}

std::vector<webrtc::SdpVideoFormat>
VideoEncoderFactory::InternalFactory::GetImplementations() const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& backend_factory : factories_) {
    for (const auto& format : backend_factory.factory->GetImplementations()) {
      formats.push_back(WithBackend(format, backend_factory.backend));
      if (IsSpecificHardwareBackend(backend_factory.backend)) {
        formats.push_back(WithBackend(format, VideoEncoderBackend::Hardware));
      }
    }
  }

  for (const auto& format : Factory().GetImplementations()) {
    formats.push_back(WithBackend(format, VideoEncoderBackend::Software));
  }

  return formats;
}

VideoEncoderFactory::CodecSupport
VideoEncoderFactory::InternalFactory::QueryCodecSupport(
    const webrtc::SdpVideoFormat& format,
    std::optional<std::string> scalability_mode) const {
  auto requested_backend = BackendFromFormat(format);
  auto stripped_format = StripBackendParameter(format);
  if (requested_backend == VideoEncoderBackend::Software) {
    auto original_format =
        webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(),
                                         stripped_format);
    auto support =
        original_format
            ? Factory().QueryCodecSupport(*original_format, scalability_mode)
            : webrtc::VideoEncoderFactory::CodecSupport{.is_supported = false};
    if (support.is_supported) {
      return support;
    }
  } else if (requested_backend &&
             *requested_backend != VideoEncoderBackend::Software &&
             *requested_backend != VideoEncoderBackend::Auto) {
    for (const auto& backend_factory : factories_) {
      if (!BackendMatches(*requested_backend, backend_factory.backend)) {
        continue;
      }

      for (const auto& supported_format :
           backend_factory.factory->GetSupportedFormats()) {
        if (stripped_format.IsSameCodec(supported_format)) {
          return webrtc::VideoEncoderFactory::CodecSupport{
              .is_supported = true,
              .is_power_efficient = true,
          };
        }
      }
    }
  }

  auto original_format =
      webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(),
                                       stripped_format);
  auto support =
      original_format
          ? Factory().QueryCodecSupport(*original_format, scalability_mode)
          : webrtc::VideoEncoderFactory::CodecSupport{.is_supported = false};
  if (support.is_supported) {
    return support;
  }

  for (const auto& backend_factory : factories_) {
    for (const auto& supported_format :
         backend_factory.factory->GetSupportedFormats()) {
      if (stripped_format.IsSameCodec(supported_format)) {
        return webrtc::VideoEncoderFactory::CodecSupport{
            .is_supported = true,
            .is_power_efficient = true,
        };
      }
    }
  }

  return support;
}

std::unique_ptr<webrtc::VideoEncoder>
VideoEncoderFactory::InternalFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  auto requested_backend = BackendFromFormat(format);
  auto stripped_format = StripBackendParameter(format);
  bool requested_backend_unavailable = false;

  if (requested_backend == VideoEncoderBackend::Software) {
    auto original_format =
        webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(),
                                         stripped_format);
    if (original_format) {
      auto encoder = Factory().Create(env, *original_format);
      if (encoder) {
        return encoder;
      }
    }
    requested_backend_unavailable = true;
  } else if (requested_backend &&
             *requested_backend != VideoEncoderBackend::Auto) {
    for (const auto& backend_factory : factories_) {
      if (!BackendMatches(*requested_backend, backend_factory.backend)) {
        continue;
      }

      for (const auto& supported_format :
           backend_factory.factory->GetSupportedFormats()) {
        if (supported_format.IsSameCodec(stripped_format)) {
          auto encoder = backend_factory.factory->Create(env, stripped_format);
          if (encoder) {
            return encoder;
          }
        }
      }
    }

    requested_backend_unavailable = true;
  }

  if (requested_backend_unavailable) {
    RTC_LOG(LS_WARNING) << "Requested video encoder backend "
                        << BackendName(*requested_backend)
                        << " is unavailable for " << stripped_format.name
                        << "; falling back to another compatible encoder.";
  }

  for (const auto& backend_factory : factories_) {
    for (const auto& supported_format :
         backend_factory.factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(stripped_format))
        return backend_factory.factory->Create(env, stripped_format);
    }
  }

  auto original_format =
      webrtc::FuzzyMatchSdpVideoFormat(Factory().GetSupportedFormats(),
                                       stripped_format);

  if (original_format) {
    return Factory().Create(env, *original_format);
  }

  RTC_LOG(LS_ERROR) << "No VideoEncoder found for " << stripped_format.name;
  return nullptr;
}

VideoEncoderFactory::VideoEncoderFactory() {
  internal_factory_ = std::make_unique<InternalFactory>();
}

std::vector<webrtc::SdpVideoFormat> VideoEncoderFactory::GetSupportedFormats()
    const {
  return internal_factory_->GetSupportedFormats();
}

std::vector<webrtc::SdpVideoFormat> VideoEncoderFactory::GetImplementations()
    const {
  return internal_factory_->GetImplementations();
}

VideoEncoderFactory::CodecSupport VideoEncoderFactory::QueryCodecSupport(
    const webrtc::SdpVideoFormat& format,
    std::optional<std::string> scalability_mode) const {
  return internal_factory_->QueryCodecSupport(format, scalability_mode);
}

std::unique_ptr<webrtc::VideoEncoder> VideoEncoderFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  std::unique_ptr<webrtc::VideoEncoder> encoder;
  if (format.IsCodecInList(internal_factory_->GetSupportedFormats())) {
    encoder = std::make_unique<webrtc::SimulcastEncoderAdapter>(
        env, internal_factory_.get(), nullptr, format);
  }

  return encoder;
}

}  // namespace livekit_ffi
