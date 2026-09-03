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

#include <algorithm>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "absl/container/inlined_vector.h"
#include "api/video/encoded_image.h"
#include "api/video/video_frame.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "api/video_codecs/video_encoder.h"
#include "av1_bitstream.h"
#include "common_video/h264/h264_common.h"
#include "livekit/encoded_video_frame_buffer.h"
#include "media/base/media_constants.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/svc/scalable_video_controller_no_layering.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit_ffi {
namespace {

using livekit::EncodedVideoFrameBuffer;
using webrtc::CodecSpecificInfo;
using webrtc::EncodedImage;
using webrtc::EncodedImageBuffer;
using webrtc::EncodedImageCallback;
using webrtc::Environment;
using webrtc::H264PacketizationMode;
using webrtc::ScalabilityMode;
using webrtc::ScalableVideoController;
using webrtc::ScalableVideoControllerNoLayering;
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
  if (format.name == "VP8") {
    return webrtc::kVideoCodecVP8;
  }
  if (format.name == "VP9") {
    return webrtc::kVideoCodecVP9;
  }
  if (format.name == "AV1") {
    return webrtc::kVideoCodecAV1;
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

bool IsAv1Codec(VideoCodecType codec_type) {
  return codec_type == webrtc::kVideoCodecAV1;
}

bool IsKeyframe(livekit::EncodedFrameType frame_type) {
  return frame_type == livekit::EncodedFrameType::kKey;
}

// SDP profile parameters constrain real encoders, not a pass-through: the
// forwarded bytes are whatever the upstream encoder produced. Match formats
// by codec only (H265/HEVC are aliases via CodecTypeFromFormat).
bool IsSameCodecType(const SdpVideoFormat& a, const SdpVideoFormat& b) {
  VideoCodecType type_a = CodecTypeFromFormat(a);
  return type_a != webrtc::kVideoCodecGeneric &&
         type_a == CodecTypeFromFormat(b);
}

void FillSingleLayerCodecSpecific(
    CodecSpecificInfo* codec_info,
    VideoCodecType codec_type,
    int width,
    int height,
    bool keyframe,
    ScalableVideoControllerNoLayering* av1_svc_controller) {
  codec_info->codecType = codec_type;
  codec_info->end_of_picture = true;

  switch (codec_type) {
    case webrtc::kVideoCodecH264:
      codec_info->codecSpecific.H264.packetization_mode =
          H264PacketizationMode::NonInterleaved;
      break;
    case webrtc::kVideoCodecVP8:
      codec_info->codecSpecific.VP8.nonReference = false;
      codec_info->codecSpecific.VP8.temporalIdx = 0;
      codec_info->codecSpecific.VP8.layerSync = false;
      codec_info->codecSpecific.VP8.keyIdx = -1;
      break;
    case webrtc::kVideoCodecVP9:
      codec_info->codecSpecific.VP9.first_frame_in_picture = true;
      codec_info->codecSpecific.VP9.inter_pic_predicted = !keyframe;
      codec_info->codecSpecific.VP9.flexible_mode = false;
      codec_info->codecSpecific.VP9.ss_data_available = keyframe;
      codec_info->codecSpecific.VP9.temporal_idx = 0;
      codec_info->codecSpecific.VP9.temporal_up_switch = true;
      codec_info->codecSpecific.VP9.inter_layer_predicted = false;
      codec_info->codecSpecific.VP9.gof_idx = 0;
      codec_info->codecSpecific.VP9.num_spatial_layers = 1;
      codec_info->codecSpecific.VP9.first_active_layer = 0;
      codec_info->codecSpecific.VP9.spatial_layer_resolution_present = keyframe;
      codec_info->codecSpecific.VP9.width[0] = width;
      codec_info->codecSpecific.VP9.height[0] = height;
      codec_info->codecSpecific.VP9.gof.SetGofInfoVP9(
          webrtc::kTemporalStructureMode1);
      codec_info->codecSpecific.VP9.num_ref_pics = keyframe ? 0 : 1;
      codec_info->codecSpecific.VP9.p_diff[0] = 1;
      break;
    case webrtc::kVideoCodecAV1: {
      codec_info->scalability_mode = ScalabilityMode::kL1T1;
      std::vector<ScalableVideoController::LayerFrameConfig> layer_frames =
          av1_svc_controller->NextFrameConfig(/*restart=*/keyframe);
      if (!layer_frames.empty()) {
        const ScalableVideoController::LayerFrameConfig& layer_frame =
            layer_frames.front();
        codec_info->generic_frame_info =
            av1_svc_controller->OnEncodeDone(layer_frame);
        if (layer_frame.IsKeyframe()) {
          codec_info->template_structure =
              av1_svc_controller->DependencyStructure();
        }
      }
      break;
    }
    default:
      break;
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
    cached_sequence_header_obu_.clear();
    av1_svc_controller_ = ScalableVideoControllerNoLayering();
    if (IsAv1Codec(codec_type_) && !codec_.GetScalabilityMode().has_value()) {
      codec_.SetScalabilityMode(ScalabilityMode::kL1T1);
    }
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
                 const std::vector<VideoFrameType>* frame_types) override {
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
    ForwardPendingRateControl(encoded_buffer);

    const bool is_keyframe = IsKeyframe(encoded_buffer->frame_type());

    // A pass-through cannot synthesize the keyframe the RTP layer wants
    // (PLI/FIR, late subscriber, reconfiguration); forward the request to
    // the capture source so the upstream encoder can produce an IDR.
    const bool keyframe_requested =
        frame_types != nullptr &&
        std::any_of(frame_types->begin(), frame_types->end(),
                    [](VideoFrameType type) {
                      return type == VideoFrameType::kVideoFrameKey;
                    });
    if (keyframe_requested && !is_keyframe) {
      encoded_buffer->request_keyframe();
    }

    if (encoded_buffer->payload_size() == 0) {
      RTC_LOG(LS_ERROR) << "PassthroughVideoEncoder received an empty frame";
      return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
    }

    // Non-AV1 payloads are forwarded without copying: the buffer already
    // owns a webrtc::EncodedImageBuffer. AV1 needs RTP normalization, which
    // may rewrite the bytes, so it works on a copy.
    webrtc::scoped_refptr<webrtc::EncodedImageBufferInterface> encoded_data;
    if (IsAv1Codec(codec_type_)) {
      std::vector<uint8_t> payload(
          encoded_buffer->payload_data(),
          encoded_buffer->payload_data() + encoded_buffer->payload_size());
      livekit::av1::NormalizeForRtp(&payload);

      std::vector<uint8_t> sequence_header;
      if (livekit::av1::ExtractSequenceHeaderObu(
              payload.data(), payload.size(), &sequence_header)) {
        cached_sequence_header_obu_ = std::move(sequence_header);
      } else if (is_keyframe && !cached_sequence_header_obu_.empty()) {
        livekit::av1::EnsureSequenceHeaderOnKeyframe(
            &payload, cached_sequence_header_obu_);
      }
      if (payload.empty() ||
          !livekit::av1::IsWebRtcParseable(payload.data(), payload.size())) {
        RTC_LOG(LS_ERROR)
            << "PassthroughVideoEncoder received an AV1 frame that WebRTC "
               "cannot packetize";
        return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
      }
      encoded_data = EncodedImageBuffer::Create(payload.data(), payload.size());
    } else {
      encoded_data = encoded_buffer->encoded_data();
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
    const size_t encoded_size = encoded_data->size();
    encoded_image.SetEncodedData(std::move(encoded_data));
    encoded_image.set_size(encoded_size);
    encoded_image.qp_ = -1;

    CodecSpecificInfo codec_info;
    codec_info.codecSpecific = {};
    FillSingleLayerCodecSpecific(&codec_info, codec_type_, encoded_buffer->width(),
                                 encoded_buffer->height(), is_keyframe,
                                 &av1_svc_controller_);

    const auto result =
        encoded_image_callback_->OnEncodedImage(encoded_image, &codec_info);
    if (result.error != EncodedImageCallback::Result::OK) {
      RTC_LOG(LS_ERROR) << "PassthroughVideoEncoder callback failed "
                        << result.error;
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    return WEBRTC_VIDEO_CODEC_OK;
  }

  void SetRates(const RateControlParameters& parameters) override {
    webrtc::MutexLock lock(&rate_control_mutex_);
    latest_rate_control_request_ = livekit::EncodedRateControlRequest{
        true, parameters.bitrate.get_sum_bps(), parameters.framerate_fps};
  }

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
  void ForwardPendingRateControl(
      EncodedVideoFrameBuffer* encoded_buffer) {
    std::optional<livekit::EncodedRateControlRequest> request;
    {
      webrtc::MutexLock lock(&rate_control_mutex_);
      request = latest_rate_control_request_;
      latest_rate_control_request_.reset();
    }
    if (request.has_value()) {
      encoded_buffer->set_rate_control_request(request->target_bitrate_bps,
                                               request->framerate_fps);
    }
  }

  Environment env_;
  SdpVideoFormat format_;
  VideoCodecType codec_type_;
  VideoCodec codec_;
  EncodedImageCallback* encoded_image_callback_ = nullptr;
  ScalableVideoControllerNoLayering av1_svc_controller_;
  std::vector<uint8_t> cached_sequence_header_obu_;
  webrtc::Mutex rate_control_mutex_;
  std::optional<livekit::EncodedRateControlRequest> latest_rate_control_request_;
};

}  // namespace

PassthroughVideoEncoderFactory::PassthroughVideoEncoderFactory() {
  std::map<std::string, std::string> h264_parameters = {
      {"profile-level-id", "42e01f"},
      {"level-asymmetry-allowed", "1"},
      {"packetization-mode", "1"},
  };
  absl::InlinedVector<ScalabilityMode, webrtc::kScalabilityModeCount>
      scalability_modes;
  scalability_modes.push_back(ScalabilityMode::kL1T1);
  supported_formats_.push_back(SdpVideoFormat::VP8());
  supported_formats_.push_back(SdpVideoFormat::VP9Profile0());
  supported_formats_.push_back(
      SdpVideoFormat(SdpVideoFormat::AV1Profile0(), scalability_modes));
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
    std::optional<std::string> scalability_mode) const {
  for (const auto& supported_format : supported_formats_) {
    if (IsSameCodecType(format, supported_format)) {
      if (format.name == "AV1" && scalability_mode.has_value() &&
          *scalability_mode != "L1T1") {
        return {.is_supported = false, .is_power_efficient = false};
      }
      return {.is_supported = true, .is_power_efficient = true};
    }
  }
  return {.is_supported = false, .is_power_efficient = false};
}

std::unique_ptr<VideoEncoder> PassthroughVideoEncoderFactory::Create(
    const Environment& env,
    const SdpVideoFormat& format) {
  // Match by codec, not by exact profile: rejecting e.g. a High-profile
  // H264 negotiation here would hand the session to a real encoder that
  // cannot consume pre-encoded frames.
  for (const auto& supported_format : supported_formats_) {
    if (IsSameCodecType(format, supported_format)) {
      return std::make_unique<PassthroughVideoEncoder>(env, format);
    }
  }
  return nullptr;
}

}  // namespace livekit_ffi
