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

#include <algorithm>
#include <atomic>
#include <cctype>
#include <cstdlib>
#include <deque>
#include <optional>
#include <string_view>
#include <utility>

#include "api/environment/environment_factory.h"
#include "api/video/video_frame.h"
#include "api/video_codecs/sdp_video_format.h"
#include "api/video_codecs/video_encoder.h"
#include "api/video_codecs/video_encoder_factory_template.h"
#include "api/video_codecs/video_encoder_software_fallback_wrapper.h"
#include "livekit/encoded_video_frame_buffer.h"
#include "livekit/objc_video_factory.h"
#include "livekit/passthrough_video_encoder.h"
#include "livekit/webrtc.h"
#include "media/base/media_constants.h"
#include "media/engine/simulcast_encoder_adapter.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

#ifdef LIVEKIT_TEST
#include "api/video/i420_buffer.h"
#include "modules/video_coding/include/video_codec_interface.h"
#endif

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

#if defined(USE_V4L2_VIDEO_CODEC)
#include "livekit/rust_video_encoder.h"
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
          (actual == VideoEncoderBackend::Hardware ||
           IsSpecificHardwareBackend(actual)));
}

bool IsAutomaticFallbackBackend(VideoEncoderBackend backend) {
  return backend != VideoEncoderBackend::PreEncoded;
}

bool EqualsIgnoreAsciiCase(std::string_view a, std::string_view b) {
  return a.size() == b.size() &&
         std::equal(a.begin(), a.end(), b.begin(), [](char x, char y) {
           return std::tolower(static_cast<unsigned char>(x)) ==
                  std::tolower(static_cast<unsigned char>(y));
         });
}

bool IsSameCodecName(std::string_view a, std::string_view b) {
  if (EqualsIgnoreAsciiCase(a, b)) {
    return true;
  }
  auto is_h265 = [](std::string_view name) {
    return EqualsIgnoreAsciiCase(name, "H265") ||
           EqualsIgnoreAsciiCase(name, "HEVC");
  };
  return is_h265(a) && is_h265(b);
}

// The pass-through backend forwards pre-encoded bytes, so SDP profile
// parameters do not constrain it: match it by codec name only. Real
// encoder backends keep exact profile matching.
bool FormatSupportedByBackendFactory(VideoEncoderBackend backend,
                                     const webrtc::SdpVideoFormat& supported,
                                     const webrtc::SdpVideoFormat& requested) {
  if (backend == VideoEncoderBackend::PreEncoded) {
    return IsSameCodecName(supported.name, requested.name);
  }
  return supported.IsSameCodec(requested);
}

void AddBackendFactory(
    std::vector<VideoEncoderBackendFactory>& factories,
    VideoEncoderBackend backend,
    std::unique_ptr<webrtc::VideoEncoderFactory> factory,
    bool software_fallback = false) {
  factories.push_back(VideoEncoderBackendFactory{backend, std::move(factory),
                                                 software_fallback});
}

void AddJetsonFactory(
    std::vector<VideoEncoderBackendFactory>& factories) {
#if defined(USE_JETSON_VIDEO_CODEC)
  if (webrtc::JetsonVideoEncoderFactory::IsSupported()) {
    AddBackendFactory(
        factories,
        VideoEncoderBackend::Hardware,
        std::make_unique<webrtc::JetsonVideoEncoderFactory>(),
        /*software_fallback=*/true);
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
        std::make_unique<webrtc::NvidiaVideoEncoderFactory>(),
        /*software_fallback=*/true);
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
        std::make_unique<webrtc::VAAPIVideoEncoderFactory>(),
        /*software_fallback=*/true);
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

// After the vendor backends, which know their hardware better.
void AddV4l2Factory(std::vector<VideoEncoderBackendFactory>& factories) {
#if defined(USE_V4L2_VIDEO_CODEC)
  if (auto factory = CreateV4l2VideoEncoderFactory()) {
    AddBackendFactory(factories, VideoEncoderBackend::Hardware,
                      std::move(factory), /*software_fallback=*/true);
  }
#else
  (void)factories;
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

namespace {

// Without the wrapper, libwebrtc only logs WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE
// and the track stops.
std::unique_ptr<webrtc::VideoEncoder> CreateBackendEncoder(
    const VideoEncoderBackendFactory& backend_factory,
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  auto encoder = backend_factory.factory->Create(env, format);
  if (!encoder || !backend_factory.software_fallback) {
    return encoder;
  }
  auto software_format = webrtc::FuzzyMatchSdpVideoFormat(
      Factory().GetSupportedFormats(), format);
  auto software_encoder =
      software_format ? Factory().Create(env, *software_format) : nullptr;
  if (!software_encoder) {
    return encoder;
  }
  return webrtc::CreateVideoEncoderSoftwareFallbackWrapper(
      env, std::move(software_encoder), std::move(encoder),
      /*prefer_temporal_support=*/false);
}

}  // namespace

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

#if defined(USE_V4L2_VIDEO_CODEC)
  if (HasV4l2VideoEncoder()) {
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

  AddV4l2Factory(factories_);
}

std::vector<webrtc::SdpVideoFormat>
VideoEncoderFactory::InternalFactory::GetSupportedFormats() const {
  std::vector<webrtc::SdpVideoFormat> formats = Factory().GetSupportedFormats();

  for (const auto& backend_factory : factories_) {
    if (backend_factory.backend == VideoEncoderBackend::PreEncoded) {
      continue;
    }
    auto supported_formats = backend_factory.factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }

  // Pre-encoded publishing forwards access units the application already
  // encoded, so it must not depend on this host having a real encoder for
  // the codec (e.g. H265 on a Linux box without a hardware encoder). Make
  // every pass-through codec negotiable by adding one entry for each codec
  // no real encoder listed. The pass-through matches by codec name at
  // Create(), so the real encoder's own entry serves it when one exists. A
  // raw sender is kept off a codec no real encoder implements by the Rust
  // publish path, which picks its codec from BackendCodecs(), and otherwise
  // by the encoder fallback described in GetImplementations().
  for (const auto& backend_factory : factories_) {
    if (backend_factory.backend != VideoEncoderBackend::PreEncoded) {
      continue;
    }
    for (const auto& format : backend_factory.factory->GetSupportedFormats()) {
      const bool codec_listed =
          std::any_of(formats.begin(), formats.end(),
                      [&](const webrtc::SdpVideoFormat& existing) {
                        return IsSameCodecName(existing.name, format.name);
                      });
      if (!codec_listed) {
        formats.push_back(format);
      }
    }
  }
  return formats;
}

std::vector<std::string> VideoEncoderFactory::InternalFactory::BackendCodecs(
    VideoEncoderBackend backend) const {
  std::vector<std::string> codecs;
  auto add_formats = [&](const std::vector<webrtc::SdpVideoFormat>& formats) {
    for (const auto& format : formats) {
      // Report one canonical name per codec; HEVC is an alias of H265.
      std::string name = format.name;
      if (IsSameCodecName(name, "H265")) {
        name = "H265";
      }
      const bool listed = std::any_of(codecs.begin(), codecs.end(),
                                      [&](const std::string& existing) {
                                        return IsSameCodecName(existing, name);
                                      });
      if (!listed) {
        codecs.push_back(std::move(name));
      }
    }
  };

  switch (backend) {
    case VideoEncoderBackend::PreEncoded:
      for (const auto& backend_factory : factories_) {
        if (backend_factory.backend == VideoEncoderBackend::PreEncoded) {
          add_formats(backend_factory.factory->GetSupportedFormats());
        }
      }
      break;
    case VideoEncoderBackend::Software:
      add_formats(Factory().GetSupportedFormats());
      break;
    case VideoEncoderBackend::Auto:
      // Everything a real encoder on this host can produce: the set Create()
      // falls back to when a requested backend is unavailable.
      for (const auto& backend_factory : factories_) {
        if (IsAutomaticFallbackBackend(backend_factory.backend)) {
          add_formats(backend_factory.factory->GetSupportedFormats());
        }
      }
      add_formats(Factory().GetSupportedFormats());
      break;
    default:
      for (const auto& backend_factory : factories_) {
        if (BackendMatches(backend, backend_factory.backend)) {
          add_formats(backend_factory.factory->GetSupportedFormats());
        }
      }
      break;
  }
  return codecs;
}

std::vector<webrtc::SdpVideoFormat>
VideoEncoderFactory::InternalFactory::GetImplementations() const {
  std::vector<webrtc::SdpVideoFormat> formats;
  for (const auto& backend_factory : factories_) {
    if (backend_factory.backend == VideoEncoderBackend::PreEncoded) {
      continue;
    }
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

  // libwebrtc only sends a negotiated codec that appears here, so a codec
  // only the pass-through implements must be listed too, or a pre-encoded
  // sender fails with "No video codecs in common". List it last and
  // untagged: real encoders keep priority for every codec they cover, and a
  // raw sender that still lands on it fails encoder creation (Create() never
  // falls back to the pass-through), letting libwebrtc switch to another
  // negotiated codec. A pre-encoded sender's selector moves it onto the
  // pass-through backend instead.
  const size_t real_format_count = formats.size();
  for (const auto& backend_factory : factories_) {
    if (backend_factory.backend != VideoEncoderBackend::PreEncoded) {
      continue;
    }
    for (const auto& format : backend_factory.factory->GetImplementations()) {
      const bool codec_implemented = std::any_of(
          formats.begin(), formats.begin() + real_format_count,
          [&](const webrtc::SdpVideoFormat& existing) {
            return IsSameCodecName(existing.name, format.name);
          });
      if (!codec_implemented) {
        formats.push_back(format);
      }
    }
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
        if (FormatSupportedByBackendFactory(backend_factory.backend,
                                            supported_format,
                                            stripped_format)) {
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
    if (!IsAutomaticFallbackBackend(backend_factory.backend)) {
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
        if (FormatSupportedByBackendFactory(backend_factory.backend,
                                            supported_format,
                                            stripped_format)) {
          auto encoder = CreateBackendEncoder(backend_factory, env,
                                              stripped_format);
          if (encoder) {
            return encoder;
          }
        }
      }
    }

    // A real encoder cannot consume the pre-encoded native frame buffers
    // this session produces, so falling back would yield a silently broken
    // sender. Fail loudly instead.
    if (*requested_backend == VideoEncoderBackend::PreEncoded) {
      RTC_LOG(LS_ERROR)
          << "Pre-encoded pass-through encoder is unavailable for "
          << stripped_format.name
          << "; refusing to fall back to a real encoder.";
      return nullptr;
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
    if (!IsAutomaticFallbackBackend(backend_factory.backend)) {
      continue;
    }
    for (const auto& supported_format :
         backend_factory.factory->GetSupportedFormats()) {
      if (!supported_format.IsSameCodec(stripped_format)) {
        continue;
      }
      auto encoder = CreateBackendEncoder(backend_factory, env,
                                          stripped_format);
      if (encoder) {
        return encoder;
      }
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

std::vector<std::string> VideoEncoderFactory::BackendCodecs(
    VideoEncoderBackend backend) const {
  return internal_factory_->BackendCodecs(backend);
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

namespace {

// Real encoders can never consume pre-encoded access units, but frames
// carrying an EncodedVideoFrameBuffer can still reach one in the window
// between stream startup and the sender's encoder selector switching onto
// the pass-through backend. Some platform encoders blind-cast native
// buffers (macOS ObjCVideoEncoder casts to ObjCFrameBuffer and retains a
// garbage pointer), so forwarding such a frame is a crash, not a graceful
// failure. Drop it instead: the selector switches shortly after, and the
// pass-through encoder requests a fresh keyframe when it takes over.
// Also preserve presentation timestamps across raw encoder backends that
// discard them. The pass-through encoder copies them directly from its input.
class VideoEncoderWrapper final : public webrtc::VideoEncoder,
                                  private webrtc::EncodedImageCallback {
 public:
  explicit VideoEncoderWrapper(std::unique_ptr<webrtc::VideoEncoder> encoder)
      : encoder_(std::move(encoder)) {}

  ~VideoEncoderWrapper() override { Release(); }

  void SetFecControllerOverride(
      webrtc::FecControllerOverride* fec_controller_override) override {
    encoder_->SetFecControllerOverride(fec_controller_override);
  }

  int InitEncode(const webrtc::VideoCodec* codec_settings,
                 const Settings& settings) override {
    return encoder_->InitEncode(codec_settings, settings);
  }

  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override {
    callback_ = callback;
    return encoder_->RegisterEncodeCompleteCallback(callback ? this : nullptr);
  }

  int32_t Release() override {
    // Release and unregister the backend before clearing local state.
    // The backend holds only a non-owning pointer to this wrapper.
    const int32_t result = encoder_->Release();
    encoder_->RegisterEncodeCompleteCallback(nullptr);
    callback_ = nullptr;
    webrtc::MutexLock lock(&timestamps_mutex_);
    timestamps_.clear();
    return result;
  }

  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* frame_types) override {
    if (livekit::EncodedVideoFrameBuffer::FromNative(
            frame.video_frame_buffer().get())) {
      static std::atomic<bool> logged{false};
      if (!logged.exchange(true)) {
        RTC_LOG(LS_WARNING)
            << "Dropping pre-encoded access unit sent to a non pass-through "
               "encoder; waiting for the sender to switch onto the "
               "pass-through backend";
      }
      return WEBRTC_VIDEO_CODEC_OK;
    }
    if (auto timestamp = frame.presentation_timestamp()) {
      webrtc::MutexLock lock(&timestamps_mutex_);
      // Retain keys for all simulcast layers, bounded even when frames drop.
      timestamps_.emplace_back(frame.rtp_timestamp(), *timestamp);
      if (timestamps_.size() > kMaxTimestampEntries) {
        timestamps_.pop_front();
      }
    }
    return encoder_->Encode(frame, frame_types);
  }

  void SetRates(const RateControlParameters& parameters) override {
    encoder_->SetRates(parameters);
  }

  void OnPacketLossRateUpdate(float packet_loss_rate) override {
    encoder_->OnPacketLossRateUpdate(packet_loss_rate);
  }

  void OnRttUpdate(int64_t rtt_ms) override { encoder_->OnRttUpdate(rtt_ms); }

  void OnLossNotification(const LossNotification& loss_notification) override {
    encoder_->OnLossNotification(loss_notification);
  }

  EncoderInfo GetEncoderInfo() const override {
    return encoder_->GetEncoderInfo();
  }

 private:
  Result OnEncodedImage(
      const webrtc::EncodedImage& image,
      const webrtc::CodecSpecificInfo* codec_specific_info) override {
    // EncodedImage copies share the encoded buffer; no payload is copied.
    webrtc::EncodedImage output = image;
    {
      webrtc::MutexLock lock(&timestamps_mutex_);
      auto it = std::find_if(timestamps_.rbegin(), timestamps_.rend(),
                             [&image](const auto& entry) {
                               return entry.first == image.RtpTimestamp();
                             });
      if (it != timestamps_.rend()) {
        output.SetPresentationTimestamp(it->second);
      }
    }
    return callback_->OnEncodedImage(output, codec_specific_info);
  }

  void OnFrameDropped(uint32_t rtp_timestamp, int spatial_id,
                      bool is_end_of_temporal_unit) override {
    callback_->OnFrameDropped(rtp_timestamp, spatial_id,
                              is_end_of_temporal_unit);
  }

  // Non-owning downstream callback registered by WebRTC.
  webrtc::EncodedImageCallback* callback_ = nullptr;
  webrtc::Mutex timestamps_mutex_;
  std::deque<std::pair<uint32_t, webrtc::Timestamp>> timestamps_;
  static constexpr size_t kMaxTimestampEntries = 300;
  // Destroy the backend before the state its callbacks can access.
  std::unique_ptr<webrtc::VideoEncoder> encoder_;
};

}  // namespace

std::unique_ptr<webrtc::VideoEncoder> VideoEncoderFactory::Create(
    const webrtc::Environment& env,
    const webrtc::SdpVideoFormat& format) {
  std::unique_ptr<webrtc::VideoEncoder> encoder;
  if (format.IsCodecInList(internal_factory_->GetSupportedFormats())) {
    encoder = std::make_unique<webrtc::SimulcastEncoderAdapter>(
        env, internal_factory_.get(), nullptr, format);
  }

  if (encoder && BackendFromFormat(format) != VideoEncoderBackend::PreEncoded) {
    encoder = std::make_unique<VideoEncoderWrapper>(std::move(encoder));
  }

  return encoder;
}

#ifdef LIVEKIT_TEST
void VideoEncoderFactory::AddBackendForTest(
    VideoEncoderBackendFactory backend) {
  internal_factory_->AddBackendForTest(std::move(backend));
}

namespace {

class FailingEncoder final : public webrtc::VideoEncoder {
 public:
  explicit FailingEncoder(bool fail_init) : fail_init_(fail_init) {}

  int InitEncode(const webrtc::VideoCodec* /* codec_settings */,
                 const Settings& /* settings */) override {
    return fail_init_ ? WEBRTC_VIDEO_CODEC_ERROR : WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t RegisterEncodeCompleteCallback(
      webrtc::EncodedImageCallback* callback) override {
    callback_ = callback;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Release() override { return WEBRTC_VIDEO_CODEC_OK; }

  int32_t Encode(
      const webrtc::VideoFrame& frame,
      const std::vector<webrtc::VideoFrameType>* /* frame_types */) override {
    if (encoded_) {
      return WEBRTC_VIDEO_CODEC_FALLBACK_SOFTWARE;
    }
    encoded_ = true;
    const uint8_t payload[] = {0};
    webrtc::EncodedImage image;
    image.SetEncodedData(
        webrtc::EncodedImageBuffer::Create(payload, sizeof(payload)));
    image.SetRtpTimestamp(frame.rtp_timestamp());
    webrtc::CodecSpecificInfo info;
    info.codecType = webrtc::kVideoCodecVP8;
    callback_->OnEncodedImage(image, &info);
    return WEBRTC_VIDEO_CODEC_OK;
  }

  void SetRates(const RateControlParameters& /* parameters */) override {}

  EncoderInfo GetEncoderInfo() const override {
    EncoderInfo info;
    info.implementation_name = "stub";
    info.is_hardware_accelerated = true;
    return info;
  }

 private:
  const bool fail_init_;
  bool encoded_ = false;
  webrtc::EncodedImageCallback* callback_ = nullptr;
};

class FailingEncoderFactory final : public webrtc::VideoEncoderFactory {
 public:
  explicit FailingEncoderFactory(uint8_t failure) : failure_(failure) {}

  std::vector<webrtc::SdpVideoFormat> GetSupportedFormats() const override {
    return {webrtc::SdpVideoFormat("VP8")};
  }

  std::unique_ptr<webrtc::VideoEncoder> Create(
      const webrtc::Environment& /* env */,
      const webrtc::SdpVideoFormat& /* format */) override {
    if (failure_ == kFallbackTestFailCreate) {
      return nullptr;
    }
    return std::make_unique<FailingEncoder>(failure_ == kFallbackTestFailInit);
  }

 private:
  const uint8_t failure_;
};

class CountingCallback final : public webrtc::EncodedImageCallback {
 public:
  Result OnEncodedImage(const webrtc::EncodedImage& /* image */,
                        const webrtc::CodecSpecificInfo* /* info */) override {
    ++sent;
    return Result(Result::OK);
  }
  void OnFrameDropped(uint32_t /* rtp_timestamp */,
                      int /* spatial_id */,
                      bool /* is_end_of_temporal_unit */) override {}
  int sent = 0;
};

}  // namespace

rust::Vec<rust::String> software_fallback_for_test(uint8_t failure) {
  VideoEncoderFactory factory;
  factory.AddBackendForTest(VideoEncoderBackendFactory{
      VideoEncoderBackend::Hardware,
      std::make_unique<FailingEncoderFactory>(failure),
      /*software_fallback=*/true});
  auto encoder = factory.Create(webrtc::CreateEnvironment(),
                                webrtc::SdpVideoFormat("VP8"));

  webrtc::VideoCodec codec;
  codec.codecType = webrtc::kVideoCodecVP8;
  codec.width = 64;
  codec.height = 64;
  codec.maxFramerate = 30;
  codec.startBitrate = 100;
  codec.maxBitrate = 200;
  codec.qpMax = 56;
  *codec.VP8() = webrtc::VideoEncoder::GetDefaultVp8Settings();
  encoder->InitEncode(
      &codec, webrtc::VideoEncoder::Settings(
                  webrtc::VideoEncoder::Capabilities(false), 1, 1200));
  CountingCallback callback;
  encoder->RegisterEncodeCompleteCallback(&callback);
  webrtc::VideoBitrateAllocation allocation;
  allocation.SetBitrate(0, 0, 100'000);
  encoder->SetRates(
      webrtc::VideoEncoder::RateControlParameters(allocation, 30.0));

  auto buffer = webrtc::I420Buffer::Create(64, 64);
  webrtc::I420Buffer::SetBlack(buffer.get());
  const std::vector<webrtc::VideoFrameType> frame_types{
      webrtc::VideoFrameType::kVideoFrameDelta};
  rust::Vec<rust::String> implementations;
  for (uint32_t i = 1; i <= 3; ++i) {
    webrtc::VideoFrame frame = webrtc::VideoFrame::Builder()
                                   .set_video_frame_buffer(buffer)
                                   .set_rtp_timestamp(i * 3000)
                                   .build();
    const int sent_before = callback.sent;
    encoder->Encode(frame, &frame_types);
    if (callback.sent > sent_before) {
      implementations.push_back(encoder->GetEncoderInfo().implementation_name);
    }
  }
  encoder->Release();
  return implementations;
}
#endif

}  // namespace livekit_ffi
