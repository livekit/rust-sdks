/*
 * Copyright 2026 LiveKit, Inc.
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

#include "livekit/passthrough_video_encoder.h"

#include <map>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "api/video/encoded_image.h"
#include "api/video/video_frame.h"
#include "api/video_codecs/video_encoder.h"
#include "common_video/h264/h264_common.h"
#include "livekit/encoded_video_frame_buffer.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "rtc_base/logging.h"

namespace livekit_ffi {
namespace {

using livekit::EncodedVideoFrameBuffer;
using webrtc::CodecSpecificInfo;
using webrtc::EncodedImage;
using webrtc::EncodedImageBuffer;
using webrtc::EncodedImageCallback;
using webrtc::Environment;
using webrtc::H264PacketizationMode;
using webrtc::SdpVideoFormat;
using webrtc::VideoCodec;
using webrtc::VideoCodecType;
using webrtc::VideoEncoder;
using webrtc::VideoFrame;
using webrtc::VideoFrameBuffer;
using webrtc::VideoFrameType;

VideoCodecType CodecTypeFromFormat(const SdpVideoFormat& format) {
  if (format.name == "H264") {
    return webrtc::kVideoCodecH264;
  }
  if (format.name == "H265" || format.name == "HEVC") {
    return webrtc::kVideoCodecH265;
  }
  return webrtc::kVideoCodecGeneric;
}

VideoCodecType CodecTypeFromBuffer(livekit::EncodedVideoCodec codec) {
  switch (codec) {
    case livekit::EncodedVideoCodec::kH264:
      return webrtc::kVideoCodecH264;
    case livekit::EncodedVideoCodec::kH265:
      return webrtc::kVideoCodecH265;
    case livekit::EncodedVideoCodec::kVP8:
      return webrtc::kVideoCodecVP8;
    case livekit::EncodedVideoCodec::kVP9:
      return webrtc::kVideoCodecVP9;
    case livekit::EncodedVideoCodec::kAV1:
      return webrtc::kVideoCodecAV1;
  }
}

VideoFrameType FrameTypeFromBuffer(livekit::EncodedFrameType frame_type) {
  switch (frame_type) {
    case livekit::EncodedFrameType::kKey:
      return VideoFrameType::kVideoFrameKey;
    case livekit::EncodedFrameType::kDelta:
      return VideoFrameType::kVideoFrameDelta;
  }
}

class PassthroughVideoEncoder final : public VideoEncoder {
 public:
  PassthroughVideoEncoder(const Environment& env, const SdpVideoFormat& format)
      : env_(env), format_(format), codec_type_(CodecTypeFromFormat(format)) {}

  int32_t InitEncode(const VideoCodec* codec_settings,
                     const Settings& /* settings */) override {
    if (!codec_settings || codec_settings->codecType != codec_type_) {
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }
    codec_ = *codec_settings;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t RegisterEncodeCompleteCallback(
      EncodedImageCallback* callback) override {
    encoded_image_callback_ = callback;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Release() override {
    encoded_image_callback_ = nullptr;
    return WEBRTC_VIDEO_CODEC_OK;
  }

  int32_t Encode(const VideoFrame& frame,
                 const std::vector<VideoFrameType>* /* frame_types */) override {
    if (!encoded_image_callback_) {
      RTC_LOG(LS_ERROR)
          << "PassthroughVideoEncoder callback is not registered";
      return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
    }

    webrtc::scoped_refptr<VideoFrameBuffer> frame_buffer =
        frame.video_frame_buffer();
    EncodedVideoFrameBuffer* encoded_buffer =
        EncodedVideoFrameBuffer::FromNative(frame_buffer.get());
    if (!encoded_buffer) {
      RTC_LOG(LS_ERROR)
          << "PassthroughVideoEncoder received a non-encoded frame buffer";
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }

    if (CodecTypeFromBuffer(encoded_buffer->codec()) != codec_type_) {
      RTC_LOG(LS_ERROR)
          << "PassthroughVideoEncoder frame codec does not match sender codec";
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }

    const std::vector<uint8_t>& payload = encoded_buffer->payload();
    if (payload.empty()) {
      RTC_LOG(LS_ERROR) << "PassthroughVideoEncoder received an empty frame";
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }

    EncodedImage encoded_image;
    encoded_image._encodedWidth = encoded_buffer->width();
    encoded_image._encodedHeight = encoded_buffer->height();
    encoded_image.SetRtpTimestamp(frame.rtp_timestamp());
    encoded_image.SetSimulcastIndex(0);
    encoded_image.ntp_time_ms_ = frame.ntp_time_ms();
    encoded_image.capture_time_ms_ = frame.render_time_ms();
    encoded_image.rotation_ = frame.rotation();
    encoded_image.content_type_ = webrtc::VideoContentType::UNSPECIFIED;
    encoded_image.timing_.flags = webrtc::VideoSendTiming::kInvalid;
    encoded_image._frameType = FrameTypeFromBuffer(encoded_buffer->frame_type());
    encoded_image.SetColorSpace(frame.color_space());
    encoded_image.SetEncodedData(
        EncodedImageBuffer::Create(payload.data(), payload.size()));
    encoded_image.set_size(payload.size());
    encoded_image.qp_ = -1;

    CodecSpecificInfo codec_info;
    codec_info.codecType = codec_type_;
    if (codec_type_ == webrtc::kVideoCodecH264) {
      codec_info.codecSpecific.H264.packetization_mode =
          H264PacketizationMode::NonInterleaved;
    }

    const auto result =
        encoded_image_callback_->OnEncodedImage(encoded_image, &codec_info);
    if (result.error != EncodedImageCallback::Result::OK) {
      RTC_LOG(LS_ERROR) << "PassthroughVideoEncoder callback failed "
                        << result.error;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    return WEBRTC_VIDEO_CODEC_OK;
  }

  void SetRates(const RateControlParameters& /* parameters */) override {}

  EncoderInfo GetEncoderInfo() const override {
    EncoderInfo info;
    info.supports_native_handle = true;
    info.implementation_name = "LiveKit pre-encoded passthrough";
    info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
    info.is_hardware_accelerated = false;
    info.supports_simulcast = false;
    info.preferred_pixel_formats = {VideoFrameBuffer::Type::kNative};
    return info;
  }

 private:
  Environment env_;
  SdpVideoFormat format_;
  VideoCodecType codec_type_;
  VideoCodec codec_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;
};

}  // namespace

PassthroughVideoEncoderFactory::PassthroughVideoEncoderFactory() {
  std::map<std::string, std::string> h264_parameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  supported_formats_.push_back(SdpVideoFormat("H264", h264_parameters));
  supported_formats_.push_back(SdpVideoFormat("H265"));
  supported_formats_.push_back(SdpVideoFormat("HEVC"));
}

std::vector<SdpVideoFormat>
PassthroughVideoEncoderFactory::GetSupportedFormats() const {
  return supported_formats_;
}

std::vector<SdpVideoFormat>
PassthroughVideoEncoderFactory::GetImplementations() const {
  return supported_formats_;
}

PassthroughVideoEncoderFactory::CodecSupport
PassthroughVideoEncoderFactory::QueryCodecSupport(
    const SdpVideoFormat& format,
    std::optional<std::string> /* scalability_mode */) const {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      return {.is_supported = true, .is_power_efficient = true};
    }
  }
  return {.is_supported = false, .is_power_efficient = false};
}

std::unique_ptr<VideoEncoder> PassthroughVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  for (const auto& supported_format : supported_formats_) {
    if (format.IsSameCodec(supported_format)) {
      return std::make_unique<PassthroughVideoEncoder>(env, supported_format);
    }
  }
  return nullptr;
}

}  // namespace livekit_ffi
