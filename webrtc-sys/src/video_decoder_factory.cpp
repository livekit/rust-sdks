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

#include "livekit/video_decoder_factory.h"

#include <modules/video_coding/codecs/av1/av1_svc_config.h>
#include "api/environment/environment.h"
#include "api/video_codecs/av1_profile.h"
#include "api/video_codecs/sdp_video_format.h"
#include "livekit/objc_video_factory.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/codecs/h264/include/h264.h"
#include "modules/video_coding/codecs/vp8/include/vp8.h"
#include "modules/video_coding/codecs/vp9/include/vp9.h"
#include "rtc_base/logging.h"

#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
#include "modules/video_coding/codecs/av1/dav1d_decoder.h"  // nogncheck
#endif

#ifdef WEBRTC_ANDROID
#include "livekit/android.h"
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
#include "nvidia/nvidia_decoder_factory.h"
#endif

#if defined(USE_JETSON_VIDEO_CODEC)
#include "jetson/jetson_decoder_factory.h"
#endif

#if defined(LIVEKIT_TEST) && defined(USE_JETSON_VIDEO_CODEC)
#include <algorithm>
#include <array>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <mutex>

#include "api/video/encoded_image.h"
#include "api/video/video_frame.h"
#include "jetson/h265_decoder_impl.h"
#include "modules/video_coding/include/video_error_codes.h"
#endif

namespace livekit_ffi {

namespace {
// H264Decoder::IsSupported() only reflects the WEBRTC_USE_H264 build flag;
// desktop prebuilts link an FFmpeg without the H.264 codec, which only
// surfaces when Configure() fails at runtime ("FFmpeg H.264 decoder not
// found"). Probe once so the SDP does not advertise decode support the
// internal decoder cannot deliver.
bool IsInternalH264DecoderAvailable() {
  if (!webrtc::H264Decoder::IsSupported()) {
    RTC_LOG(LS_WARNING) << "Internal H264 decoder not compiled in "
                           "(WEBRTC_USE_H264 off)";
    return false;
  }
  auto decoder = webrtc::H264Decoder::Create();
  if (!decoder) {
    RTC_LOG(LS_WARNING) << "H264Decoder::Create() returned null";
    return false;
  }
  webrtc::VideoDecoder::Settings settings;
  settings.set_codec_type(webrtc::kVideoCodecH264);
  if (!decoder->Configure(settings)) {
    RTC_LOG(LS_WARNING) << "Internal H264 decoder failed to configure; "
                           "FFmpeg likely lacks the H.264 codec";
    return false;
  }
  return true;
}
}  // namespace

VideoDecoderFactory::VideoDecoderFactory()
    : internal_h264_decoder_works_(IsInternalH264DecoderAvailable()) {
#ifdef __APPLE__
  factories_.push_back(livekit_ffi::CreateObjCVideoDecoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  factories_.push_back(CreateAndroidVideoDecoderFactory());
#endif

#if defined(USE_JETSON_VIDEO_CODEC)
  if (webrtc::JetsonVideoDecoderFactory::IsSupported()) {
    factories_.push_back(std::make_unique<webrtc::JetsonVideoDecoderFactory>());
  }
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
  if (webrtc::NvidiaVideoDecoderFactory::IsSupported()) {
    factories_.push_back(std::make_unique<webrtc::NvidiaVideoDecoderFactory>());
  }
#endif

  if (!internal_h264_decoder_works_) {
    RTC_LOG(LS_WARNING) << "Internal H264 decoder is unavailable, "
                           "not advertising its formats";
  }
}

std::vector<webrtc::SdpVideoFormat> VideoDecoderFactory::GetSupportedFormats()
    const {
  std::vector<webrtc::SdpVideoFormat> formats;

  for (const auto& factory : factories_) {
    auto supported_formats = factory->GetSupportedFormats();
    formats.insert(formats.end(), supported_formats.begin(),
                   supported_formats.end());
  }

  formats.push_back(webrtc::SdpVideoFormat(webrtc::kVp8CodecName));
  for (const webrtc::SdpVideoFormat& format :
       webrtc::SupportedVP9DecoderCodecs())
    formats.push_back(format);
  if (internal_h264_decoder_works_) {
    for (const webrtc::SdpVideoFormat& h264_format :
         webrtc::SupportedH264DecoderCodecs()) {
      formats.push_back(h264_format);
    }
  }

  formats.push_back(webrtc::SdpVideoFormat(
      webrtc::SdpVideoFormat::AV1Profile0(),
      webrtc::LibaomAv1EncoderSupportedScalabilityModes()));
  return formats;
}

VideoDecoderFactory::CodecSupport VideoDecoderFactory::QueryCodecSupport(
    const webrtc::SdpVideoFormat& format,
    bool reference_scaling) const {
  if (reference_scaling) {
    webrtc::VideoCodecType codec =
        webrtc::PayloadStringToCodecType(format.name);
    if (codec != webrtc::kVideoCodecVP9 && codec != webrtc::kVideoCodecAV1) {
      return {/*is_supported=*/false, /*is_power_efficient=*/false};
    }
  }

  CodecSupport codec_support;
  codec_support.is_supported = format.IsCodecInList(GetSupportedFormats());
  return codec_support;
}

std::unique_ptr<webrtc::VideoDecoder> VideoDecoderFactory::Create(
    const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) {
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format))
        return factory->Create(env, format);
    }
  }

  // IsSameCodec treats H.264 packetization-modes as distinct codecs, so when
  // the SFU sends mode=0 but the platform factory only advertises mode=1 the
  // strict match above fails. Retry with the factory's packetization-mode so
  // only that parameter is relaxed while the profile-level-id check is kept.
  if (absl::EqualsIgnoreCase(format.name, webrtc::kH264CodecName)) {
    for (const auto& factory : factories_) {
      for (const auto& sf : factory->GetSupportedFormats()) {
        if (!absl::EqualsIgnoreCase(sf.name, webrtc::kH264CodecName))
          continue;
        auto adjusted = format;
        auto it = sf.parameters.find("packetization-mode");
        if (it != sf.parameters.end())
          adjusted.parameters["packetization-mode"] = it->second;
        else
          adjusted.parameters.erase("packetization-mode");
        if (sf.IsSameCodec(adjusted))
          return factory->Create(env, adjusted);
      }
    }
  }

  if (absl::EqualsIgnoreCase(format.name, webrtc::kVp8CodecName))
    return webrtc::CreateVp8Decoder(env);
  if (absl::EqualsIgnoreCase(format.name, webrtc::kVp9CodecName))
    return webrtc::VP9Decoder::Create();
  if (absl::EqualsIgnoreCase(format.name, webrtc::kH264CodecName) &&
      internal_h264_decoder_works_)
    return webrtc::H264Decoder::Create();


#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
  if (absl::EqualsIgnoreCase(format.name, webrtc::kAv1CodecName)) {
    return webrtc::CreateDav1dDecoder();
  }
#endif


  RTC_LOG(LS_ERROR) << "No VideoDecoder found for " << format.name;
  return nullptr;
}

#ifdef LIVEKIT_TEST
#if defined(USE_JETSON_VIDEO_CODEC)
namespace {

class RecordingDecodedCallback final : public webrtc::DecodedImageCallback {
 public:
  int32_t Decoded(webrtc::VideoFrame& frame) override {
    std::lock_guard<std::mutex> lock(mutex_);
    pictures_.push_back({static_cast<uint32_t>(frame.width()),
                         static_cast<uint32_t>(frame.height()),
                         frame.rtp_timestamp()});
    arrived_.notify_all();
    return WEBRTC_VIDEO_CODEC_OK;
  }

  // Pictures arrive in decode order, so waiting for the last one waits for
  // all of them.
  void WaitFor(uint32_t rtp_timestamp) {
    std::unique_lock<std::mutex> lock(mutex_);
    arrived_.wait_for(lock, std::chrono::seconds(5), [&] {
      return std::any_of(pictures_.begin(), pictures_.end(),
                         [&](const auto& p) { return p[2] == rtp_timestamp; });
    });
  }

  std::vector<std::array<uint32_t, 3>> Pictures() {
    std::lock_guard<std::mutex> lock(mutex_);
    return pictures_;
  }

 private:
  std::mutex mutex_;
  std::condition_variable arrived_;
  std::vector<std::array<uint32_t, 3>> pictures_;
};

bool IsH265Keyframe(const std::vector<uint8_t>& access_unit) {
  for (size_t i = 0; i + 3 < access_unit.size(); ++i) {
    if (access_unit[i] == 0 && access_unit[i + 1] == 0 &&
        access_unit[i + 2] == 1) {
      const int nal_type = (access_unit[i + 3] >> 1) & 0x3f;
      if (nal_type >= 16 && nal_type <= 23) {
        return true;
      }
    }
  }
  return false;
}

}  // namespace
#endif

rust::Vec<uint32_t> jetson_h265_decode_for_test(
    rust::Slice<const uint8_t> access_units,
    uint32_t corrupt_index,
    bool release_immediately) {
  rust::Vec<uint32_t> result;
#if defined(USE_JETSON_VIDEO_CODEC)
  // Surface decoder warnings in test output.
  webrtc::LogMessage::LogToDebug(webrtc::LS_WARNING);
  if (!webrtc::JetsonVideoDecoderFactory::IsSupported()) {
    result.push_back(kJetsonDecodeUnsupported);
    return result;
  }

  RecordingDecodedCallback callback;
  uint32_t accepted = 0;
  uint32_t rejected = 0;
  {
    webrtc::JetsonH265DecoderImpl decoder;
    decoder.RegisterDecodeCompleteCallback(&callback);
    webrtc::VideoDecoder::Settings settings;
    settings.set_codec_type(webrtc::kVideoCodecH265);
    settings.set_max_render_resolution({1920, 1080});
    if (!decoder.Configure(settings)) {
      result.push_back(kJetsonDecodeConfigureFailed);
      return result;
    }

    uint32_t last_accepted_rtp = 0;
    size_t offset = 0;
    for (uint32_t index = 0; offset + 4 <= access_units.size(); ++index) {
      uint32_t size = 0;
      std::memcpy(&size, access_units.data() + offset, sizeof(size));
      offset += sizeof(size);
      if (size > access_units.size() - offset) {
        break;
      }
      std::vector<uint8_t> access_unit(access_units.data() + offset,
                                       access_units.data() + offset + size);
      offset += size;

      const bool keyframe = IsH265Keyframe(access_unit);
      if (index == corrupt_index) {
        // Keep the leading start code and NAL header so it still reads as
        // H.265, and scramble the slice data after it.
        for (size_t i = 8; i < access_unit.size(); ++i) {
          access_unit[i] ^= 0x5a;
        }
      }

      const uint32_t rtp_timestamp = (index + 1) * 3000;
      webrtc::EncodedImage image;
      image.SetEncodedData(webrtc::EncodedImageBuffer::Create(
          access_unit.data(), access_unit.size()));
      image.SetRtpTimestamp(rtp_timestamp);
      image.SetFrameType(keyframe ? webrtc::VideoFrameType::kVideoFrameKey
                                  : webrtc::VideoFrameType::kVideoFrameDelta);
      if (decoder.Decode(image, false, 0) == WEBRTC_VIDEO_CODEC_OK) {
        ++accepted;
        last_accepted_rtp = rtp_timestamp;
      } else {
        ++rejected;
      }
    }

    if (!release_immediately && accepted > 0) {
      callback.WaitFor(last_accepted_rtp);
    }
    decoder.Release();
  }

  result.push_back(kJetsonDecodeOk);
  result.push_back(accepted);
  result.push_back(rejected);
  for (const auto& picture : callback.Pictures()) {
    for (uint32_t value : picture) {
      result.push_back(value);
    }
  }
#else
  (void)access_units;
  (void)corrupt_index;
  (void)release_immediately;
  result.push_back(kJetsonDecodeUnsupported);
#endif
  return result;
}
#endif

}  // namespace livekit_ffi
