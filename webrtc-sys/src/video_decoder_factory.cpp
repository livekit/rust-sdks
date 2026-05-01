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

#include <algorithm>
#include <cstdint>
#include <iomanip>
#include <memory>
#include <optional>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

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

#if defined(RTC_ENABLE_H265)
#include "common_video/h265/h265_common.h"
#endif

#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
#include "modules/video_coding/codecs/av1/dav1d_decoder.h"  // nogncheck
#endif

#ifdef WEBRTC_ANDROID
#include "livekit/android.h"
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
#include "nvidia/nvidia_decoder_factory.h"
#endif

namespace livekit_ffi {
namespace {

bool IsH265Format(const webrtc::SdpVideoFormat& format) {
  return absl::EqualsIgnoreCase(format.name, webrtc::kH265CodecName) ||
         absl::EqualsIgnoreCase(format.name, "HEVC");
}

std::string FormatSdpVideoFormat(const webrtc::SdpVideoFormat& format) {
  std::ostringstream oss;
  oss << format.name;
  if (!format.parameters.empty()) {
    oss << " {";
    bool first = true;
    for (const auto& param : format.parameters) {
      if (!first)
        oss << ", ";
      first = false;
      oss << param.first << "=" << param.second;
    }
    oss << "}";
  }
  return oss.str();
}

#if defined(RTC_ENABLE_H265)

const char* VideoFrameTypeName(webrtc::VideoFrameType type) {
  switch (type) {
    case webrtc::VideoFrameType::kEmptyFrame:
      return "empty";
    case webrtc::VideoFrameType::kVideoFrameKey:
      return "key";
    case webrtc::VideoFrameType::kVideoFrameDelta:
      return "delta";
  }
  return "unknown";
}

std::string H265NaluTypeName(webrtc::H265::NaluType type) {
  switch (type) {
    case webrtc::H265::NaluType::kTrailN:
      return "TRAIL_N";
    case webrtc::H265::NaluType::kTrailR:
      return "TRAIL_R";
    case webrtc::H265::NaluType::kTsaN:
      return "TSA_N";
    case webrtc::H265::NaluType::kTsaR:
      return "TSA_R";
    case webrtc::H265::NaluType::kStsaN:
      return "STSA_N";
    case webrtc::H265::NaluType::kStsaR:
      return "STSA_R";
    case webrtc::H265::NaluType::kRadlN:
      return "RADL_N";
    case webrtc::H265::NaluType::kRadlR:
      return "RADL_R";
    case webrtc::H265::NaluType::kRaslN:
      return "RASL_N";
    case webrtc::H265::NaluType::kRaslR:
      return "RASL_R";
    case webrtc::H265::NaluType::kBlaWLp:
      return "BLA_W_LP";
    case webrtc::H265::NaluType::kBlaWRadl:
      return "BLA_W_RADL";
    case webrtc::H265::NaluType::kBlaNLp:
      return "BLA_N_LP";
    case webrtc::H265::NaluType::kIdrWRadl:
      return "IDR_W_RADL";
    case webrtc::H265::NaluType::kIdrNLp:
      return "IDR_N_LP";
    case webrtc::H265::NaluType::kCra:
      return "CRA";
    case webrtc::H265::NaluType::kRsvIrapVcl23:
      return "RSV_IRAP_VCL23";
    case webrtc::H265::NaluType::kRsvVcl31:
      return "RSV_VCL31";
    case webrtc::H265::NaluType::kVps:
      return "VPS";
    case webrtc::H265::NaluType::kSps:
      return "SPS";
    case webrtc::H265::NaluType::kPps:
      return "PPS";
    case webrtc::H265::NaluType::kAud:
      return "AUD";
    case webrtc::H265::NaluType::kPrefixSei:
      return "PREFIX_SEI";
    case webrtc::H265::NaluType::kSuffixSei:
      return "SUFFIX_SEI";
    case webrtc::H265::NaluType::kAp:
      return "AP";
    case webrtc::H265::NaluType::kFu:
      return "FU";
    case webrtc::H265::NaluType::kPaci:
      return "PACI";
  }

  return "type(" + std::to_string(static_cast<int>(type)) + ")";
}

bool IsH265Irap(webrtc::H265::NaluType type) {
  return type >= webrtc::H265::NaluType::kBlaWLp &&
         type <= webrtc::H265::NaluType::kRsvIrapVcl23;
}

std::string HexPrefix(const uint8_t* data, size_t size, size_t max_bytes) {
  std::ostringstream oss;
  const size_t bytes = std::min(size, max_bytes);
  for (size_t i = 0; i < bytes; ++i) {
    if (i > 0)
      oss << " ";
    oss << std::hex << std::setw(2) << std::setfill('0')
        << static_cast<int>(data[i]);
  }
  if (size > bytes)
    oss << " ...";
  return oss.str();
}

struct H265BitstreamSummary {
  size_t nal_units = 0;
  bool has_vps = false;
  bool has_sps = false;
  bool has_pps = false;
  bool has_irap = false;
  bool has_vcl = false;
  std::string types;
};

H265BitstreamSummary SummarizeH265Bitstream(const uint8_t* data, size_t size) {
  H265BitstreamSummary summary;
  if (data == nullptr || size == 0)
    return summary;

  const std::vector<webrtc::H265::NaluIndex> nalu_indices =
      webrtc::H265::FindNaluIndices(data, size);
  summary.nal_units = nalu_indices.size();

  constexpr size_t kMaxTypesToLog = 12;
  for (size_t i = 0; i < nalu_indices.size(); ++i) {
    const auto& nalu = nalu_indices[i];
    if (nalu.payload_size < webrtc::H265::kNaluHeaderSize)
      continue;

    const auto type = webrtc::H265::ParseNaluType(data[nalu.payload_start_offset]);
    summary.has_vps = summary.has_vps || type == webrtc::H265::NaluType::kVps;
    summary.has_sps = summary.has_sps || type == webrtc::H265::NaluType::kSps;
    summary.has_pps = summary.has_pps || type == webrtc::H265::NaluType::kPps;
    summary.has_irap = summary.has_irap || IsH265Irap(type);
    summary.has_vcl =
        summary.has_vcl || type <= webrtc::H265::NaluType::kRsvVcl31;

    if (i < kMaxTypesToLog) {
      if (!summary.types.empty())
        summary.types += ",";
      summary.types += H265NaluTypeName(type);
    } else if (i == kMaxTypesToLog) {
      summary.types += ",...";
    }
  }

  return summary;
}

class H265LoggingVideoDecoder;

class H265LoggingDecodedImageCallback final
    : public webrtc::DecodedImageCallback {
 public:
  H265LoggingDecodedImageCallback(H265LoggingVideoDecoder* owner,
                                  webrtc::DecodedImageCallback* callback)
      : owner_(owner), callback_(callback) {}

  int32_t Decoded(webrtc::VideoFrame& decoded_image) override;
  int32_t Decoded(webrtc::VideoFrame& decoded_image,
                  int64_t decode_time_ms) override;
  void Decoded(webrtc::VideoFrame& decoded_image,
               std::optional<int32_t> decode_time_ms,
               std::optional<uint8_t> qp) override;

 private:
  H265LoggingVideoDecoder* owner_;
  webrtc::DecodedImageCallback* callback_;
};

class H265LoggingVideoDecoder final : public webrtc::VideoDecoder {
 public:
  H265LoggingVideoDecoder(std::unique_ptr<webrtc::VideoDecoder> decoder,
                          webrtc::SdpVideoFormat format,
                          std::string source)
      : decoder_(std::move(decoder)),
        format_(std::move(format)),
        source_(std::move(source)) {
    RTC_LOG(LS_INFO) << "H265 subscriber decoder diagnostics enabled for "
                     << FormatSdpVideoFormat(format_) << " via " << source_
                     << " implementation="
                     << decoder_->GetDecoderInfo().implementation_name;
  }

  bool Configure(const Settings& settings) override {
    const webrtc::RenderResolution resolution = settings.max_render_resolution();
    RTC_LOG(LS_INFO) << "Configuring H265 decoder: codec_type="
                     << settings.codec_type()
                     << " max_render_resolution="
                     << (resolution.Valid() ? std::to_string(resolution.Width()) +
                                                   "x" +
                                                   std::to_string(resolution.Height())
                                             : "unset")
                     << " source=" << source_;
    return decoder_->Configure(settings);
  }

  int32_t Decode(const webrtc::EncodedImage& input_image,
                 int64_t render_time_ms) override {
    LogDecodeInput(input_image, false, render_time_ms);
    const int32_t result = decoder_->Decode(input_image, render_time_ms);
    LogDecodeResult(result);
    return result;
  }

  int32_t Decode(const webrtc::EncodedImage& input_image,
                 bool missing_frames,
                 int64_t render_time_ms) override {
    LogDecodeInput(input_image, missing_frames, render_time_ms);
    const int32_t result =
        decoder_->Decode(input_image, missing_frames, render_time_ms);
    LogDecodeResult(result);
    return result;
  }

  int32_t RegisterDecodeCompleteCallback(
      webrtc::DecodedImageCallback* callback) override {
    if (callback == nullptr) {
      decoded_callback_.reset();
      RTC_LOG(LS_INFO) << "H265 decoder callback cleared";
      return decoder_->RegisterDecodeCompleteCallback(nullptr);
    }

    decoded_callback_ =
        std::make_unique<H265LoggingDecodedImageCallback>(this, callback);
    RTC_LOG(LS_INFO) << "H265 decoder callback registered";
    return decoder_->RegisterDecodeCompleteCallback(decoded_callback_.get());
  }

  int32_t Release() override {
    RTC_LOG(LS_INFO) << "Releasing H265 decoder diagnostics: encoded_frames="
                     << encoded_frames_ << " decoded_frames=" << decoded_frames_;
    return decoder_->Release();
  }

  DecoderInfo GetDecoderInfo() const override {
    return decoder_->GetDecoderInfo();
  }

  const char* ImplementationName() const override {
    return decoder_->ImplementationName();
  }

  void LogDecodedFrame(const webrtc::VideoFrame& decoded_image,
                       const char* callback_variant) {
    ++decoded_frames_;
    const bool should_log =
        decoded_frames_ <= 5 || decoded_frames_ % 120 == 0;
    if (!should_log)
      return;

    RTC_LOG(LS_INFO) << "H265 decoded frame #" << decoded_frames_
                     << " via " << callback_variant << ": "
                     << decoded_image.width() << "x" << decoded_image.height()
                     << " rtp_ts=" << decoded_image.rtp_timestamp()
                     << " timestamp_us=" << decoded_image.timestamp_us();
  }

 private:
  void LogDecodeInput(const webrtc::EncodedImage& input_image,
                      bool missing_frames,
                      int64_t render_time_ms) {
    ++encoded_frames_;
    const H265BitstreamSummary summary =
        SummarizeH265Bitstream(input_image.data(), input_image.size());
    const bool keyframe =
        input_image.FrameType() == webrtc::VideoFrameType::kVideoFrameKey;
    const bool suspicious =
        input_image.size() > 0 &&
        (summary.nal_units == 0 ||
         (keyframe && (!summary.has_irap || !summary.has_vps ||
                       !summary.has_sps || !summary.has_pps)));
    const bool should_log =
        encoded_frames_ <= 5 || encoded_frames_ % 120 == 0 || keyframe ||
        suspicious;
    if (!should_log)
      return;

    std::ostringstream oss;
    oss << "H265 decode input #" << encoded_frames_
        << ": bytes=" << input_image.size()
        << " rtp_ts=" << input_image.RtpTimestamp()
        << " frame_type=" << VideoFrameTypeName(input_image.FrameType())
        << " encoded_size=" << input_image._encodedWidth << "x"
        << input_image._encodedHeight << " render_time_ms=" << render_time_ms
        << " missing_frames=" << missing_frames
        << " nals=" << summary.nal_units << " types=[" << summary.types << "]"
        << " vps/sps/pps/irap/vcl=" << summary.has_vps << "/"
        << summary.has_sps << "/" << summary.has_pps << "/" << summary.has_irap
        << "/" << summary.has_vcl;
    if (summary.nal_units == 0 && input_image.data() != nullptr) {
      oss << " first_bytes=" << HexPrefix(input_image.data(), input_image.size(), 16);
    }

    if (suspicious) {
      RTC_LOG(LS_WARNING) << oss.str();
    } else {
      RTC_LOG(LS_INFO) << oss.str();
    }
  }

  void LogDecodeResult(int32_t result) {
    if (result != 0) {
      RTC_LOG(LS_ERROR) << "H265 decoder returned error " << result
                        << " after encoded frame #" << encoded_frames_;
    }
  }

  std::unique_ptr<webrtc::VideoDecoder> decoder_;
  webrtc::SdpVideoFormat format_;
  std::string source_;
  std::unique_ptr<H265LoggingDecodedImageCallback> decoded_callback_;
  uint64_t encoded_frames_ = 0;
  uint64_t decoded_frames_ = 0;
};

int32_t H265LoggingDecodedImageCallback::Decoded(
    webrtc::VideoFrame& decoded_image) {
  owner_->LogDecodedFrame(decoded_image, "Decoded(frame)");
  return callback_->Decoded(decoded_image);
}

int32_t H265LoggingDecodedImageCallback::Decoded(
    webrtc::VideoFrame& decoded_image,
    int64_t decode_time_ms) {
  owner_->LogDecodedFrame(decoded_image, "Decoded(frame, decode_time_ms)");
  return callback_->Decoded(decoded_image, decode_time_ms);
}

void H265LoggingDecodedImageCallback::Decoded(
    webrtc::VideoFrame& decoded_image,
    std::optional<int32_t> decode_time_ms,
    std::optional<uint8_t> qp) {
  owner_->LogDecodedFrame(decoded_image, "Decoded(frame, decode_time, qp)");
  callback_->Decoded(decoded_image, decode_time_ms, qp);
}

std::unique_ptr<webrtc::VideoDecoder> WrapH265DecoderForDiagnostics(
    std::unique_ptr<webrtc::VideoDecoder> decoder,
    const webrtc::SdpVideoFormat& format,
    std::string source) {
  if (!decoder || !IsH265Format(format))
    return decoder;

  return std::make_unique<H265LoggingVideoDecoder>(std::move(decoder), format,
                                                   std::move(source));
}

#else

std::unique_ptr<webrtc::VideoDecoder> WrapH265DecoderForDiagnostics(
    std::unique_ptr<webrtc::VideoDecoder> decoder,
    const webrtc::SdpVideoFormat& /*format*/,
    std::string /*source*/) {
  return decoder;
}

#endif

}  // namespace

VideoDecoderFactory::VideoDecoderFactory() {
#ifdef __APPLE__
  factories_.push_back(livekit_ffi::CreateObjCVideoDecoderFactory());
#endif

#ifdef WEBRTC_ANDROID
  factories_.push_back(CreateAndroidVideoDecoderFactory());
#endif

#if defined(USE_NVIDIA_VIDEO_CODEC)
  if (webrtc::NvidiaVideoDecoderFactory::IsSupported()) {
    factories_.push_back(std::make_unique<webrtc::NvidiaVideoDecoderFactory>());
  }
#endif
  RTC_LOG(LS_INFO) << "LiveKit VideoDecoderFactory initialized with "
                   << factories_.size() << " platform/hardware factories";
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
  for (const webrtc::SdpVideoFormat& h264_format :
       webrtc::SupportedH264DecoderCodecs())
    formats.push_back(h264_format);

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
  if (IsH265Format(format)) {
    RTC_LOG(LS_INFO) << "Query decoder support for "
                     << FormatSdpVideoFormat(format)
                     << " reference_scaling=" << reference_scaling
                     << " supported=" << codec_support.is_supported
                     << " power_efficient="
                     << codec_support.is_power_efficient;
  }
  return codec_support;
}

std::unique_ptr<webrtc::VideoDecoder> VideoDecoderFactory::Create(
    const webrtc::Environment& env, const webrtc::SdpVideoFormat& format) {
  RTC_LOG(LS_INFO) << "VideoDecoderFactory::Create requested "
                   << FormatSdpVideoFormat(format);
  for (const auto& factory : factories_) {
    for (const auto& supported_format : factory->GetSupportedFormats()) {
      if (supported_format.IsSameCodec(format)) {
        auto decoder = factory->Create(env, format);
        if (decoder) {
          RTC_LOG(LS_INFO) << "Selected platform/hardware decoder for "
                           << FormatSdpVideoFormat(format) << " matched "
                           << FormatSdpVideoFormat(supported_format)
                           << " implementation="
                           << decoder->GetDecoderInfo().implementation_name;
          return WrapH265DecoderForDiagnostics(std::move(decoder), format,
                                               "platform/hardware");
        }
        RTC_LOG(LS_WARNING) << "Platform/hardware decoder factory matched "
                            << FormatSdpVideoFormat(supported_format)
                            << " but returned null for "
                            << FormatSdpVideoFormat(format);
      }
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
        if (sf.IsSameCodec(adjusted)) {
          auto decoder = factory->Create(env, adjusted);
          if (decoder) {
            RTC_LOG(LS_INFO)
                << "Selected platform/hardware H264 decoder after "
                   "packetization-mode adjustment: requested "
                << FormatSdpVideoFormat(format) << " adjusted "
                << FormatSdpVideoFormat(adjusted);
            return decoder;
          }
        }
      }
    }
  }

  if (absl::EqualsIgnoreCase(format.name, webrtc::kVp8CodecName)) {
    RTC_LOG(LS_INFO) << "Selected built-in VP8 decoder";
    return webrtc::CreateVp8Decoder(env);
  }
  if (absl::EqualsIgnoreCase(format.name, webrtc::kVp9CodecName)) {
    RTC_LOG(LS_INFO) << "Selected built-in VP9 decoder";
    return webrtc::VP9Decoder::Create();
  }
  if (absl::EqualsIgnoreCase(format.name, webrtc::kH264CodecName)) {
    RTC_LOG(LS_INFO) << "Selected built-in H264 decoder";
    return webrtc::H264Decoder::Create();
  }


#if defined(RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY)
  if (absl::EqualsIgnoreCase(format.name, webrtc::kAv1CodecName)) {
    RTC_LOG(LS_INFO) << "Selected built-in dav1d AV1 decoder";
    return webrtc::CreateDav1dDecoder();
  }
#endif


  RTC_LOG(LS_ERROR) << "No VideoDecoder found for "
                    << FormatSdpVideoFormat(format);
  return nullptr;
}

}  // namespace livekit_ffi
