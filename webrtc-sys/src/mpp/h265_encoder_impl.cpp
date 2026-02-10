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

#include "h265_encoder_impl.h"

#include <algorithm>
#include <cstring>
#include <limits>
#include <string>

#include "api/video/video_codec_constants.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"
#include "third_party/libyuv/include/libyuv/convert.h"

// MPP alignment macros
#define MPP_ALIGN(x, a) (((x) + (a) - 1) & ~((a) - 1))

namespace webrtc {

MppH265EncoderImpl::MppH265EncoderImpl(const webrtc::Environment& env,
                                       const SdpVideoFormat& format)
    : env_(env), format_(format) {}

MppH265EncoderImpl::~MppH265EncoderImpl() {
  Release();
}

void MppH265EncoderImpl::ReportInit() {
  if (has_reported_init_)
    return;
  has_reported_init_ = true;
}

void MppH265EncoderImpl::ReportError() {
  if (has_reported_error_)
    return;
  has_reported_error_ = true;
}

int32_t MppH265EncoderImpl::InitMpp() {
  MPP_RET ret = MPP_OK;

  ret = mpp_create(&mpp_ctx_, &mpp_api_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "mpp_create failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  ret = mpp_init(mpp_ctx_, MPP_CTX_ENC, MPP_VIDEO_CodingHEVC);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "mpp_init for H.265 encoder failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH265EncoderImpl::ConfigureMpp() {
  MPP_RET ret = MPP_OK;

  ret = mpp_enc_cfg_init(&mpp_cfg_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "mpp_enc_cfg_init failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  ret = mpp_api_->control(mpp_ctx_, MPP_ENC_GET_CFG, mpp_cfg_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "MPP_ENC_GET_CFG failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // ---- Prep config (input frame format) ----
  mpp_enc_cfg_set_s32(mpp_cfg_, "prep:width", codec_.width);
  mpp_enc_cfg_set_s32(mpp_cfg_, "prep:height", codec_.height);
  mpp_enc_cfg_set_s32(mpp_cfg_, "prep:hor_stride", hor_stride_);
  mpp_enc_cfg_set_s32(mpp_cfg_, "prep:ver_stride", ver_stride_);
  mpp_enc_cfg_set_s32(mpp_cfg_, "prep:format", MPP_FMT_YUV420P);

  // ---- Rate control config ----
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:mode", MPP_ENC_RC_MODE_CBR);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_target", configuration_.target_bps);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_max",
                      configuration_.target_bps * 3 / 2);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_min", configuration_.target_bps / 2);

  int fps_num = static_cast<int>(configuration_.max_frame_rate);
  if (fps_num < 1) fps_num = 30;
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_in_flex", 0);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_in_num", fps_num);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_in_denorm", 1);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_out_flex", 0);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_out_num", fps_num);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_out_denorm", 1);

  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:gop", fps_num * 10);

  // ---- H.265 codec config ----
  mpp_enc_cfg_set_s32(mpp_cfg_, "codec:id", MPP_VIDEO_CodingHEVC);

  // QP range
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_init", 26);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_max", 51);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_min", 10);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_max_i", 46);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_min_i", 10);
  mpp_enc_cfg_set_s32(mpp_cfg_, "rc:qp_delta_ip", 6);

  ret = mpp_api_->control(mpp_ctx_, MPP_ENC_SET_CFG, mpp_cfg_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "MPP_ENC_SET_CFG failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Set header mode: attach VPS/SPS/PPS on each IDR
  MppEncHeaderMode header_mode = MPP_ENC_HEADER_MODE_EACH_IDR;
  ret = mpp_api_->control(mpp_ctx_, MPP_ENC_SET_HEADER_MODE, &header_mode);
  if (ret != MPP_OK) {
    RTC_LOG(LS_WARNING) << "MPP_ENC_SET_HEADER_MODE failed: " << ret;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH265EncoderImpl::InitEncode(const VideoCodec* inst,
                                       const VideoEncoder::Settings& settings) {
  if (!inst || inst->codecType != kVideoCodecH265) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return release_ret;
  }

  codec_ = *inst;

  if (codec_.numberOfSimulcastStreams == 0) {
    codec_.simulcastStream[0].width = codec_.width;
    codec_.simulcastStream[0].height = codec_.height;
  }

  // Calculate aligned strides for MPP
  hor_stride_ = MPP_ALIGN(codec_.width, 16);
  ver_stride_ = MPP_ALIGN(codec_.height, 16);
  frame_size_ = hor_stride_ * ver_stride_ * 3 / 2;

  const size_t new_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(new_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval = 0;
  configuration_.width = codec_.width;
  configuration_.height = codec_.height;
  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  int32_t mpp_ret = InitMpp();
  if (mpp_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return mpp_ret;
  }

  mpp_ret = ConfigureMpp();
  if (mpp_ret != WEBRTC_VIDEO_CODEC_OK) {
    ReportError();
    return mpp_ret;
  }

  // Allocate MPP buffers
  MPP_RET ret = mpp_buffer_group_get_internal(&frame_group_, MPP_BUFFER_TYPE_DRM);
  if (ret != MPP_OK) {
    ret = mpp_buffer_group_get_internal(&frame_group_, MPP_BUFFER_TYPE_ION);
    if (ret != MPP_OK) {
      RTC_LOG(LS_ERROR) << "Failed to get MPP buffer group: " << ret;
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  ret = mpp_buffer_get(frame_group_, &frame_buf_, frame_size_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to allocate MPP frame buffer: " << ret;
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  size_t pkt_size = codec_.width * codec_.height;
  ret = mpp_buffer_get(frame_group_, &pkt_buf_, pkt_size);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to allocate MPP packet buffer: " << ret;
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  RTC_LOG(LS_INFO) << "Rockchip MPP H265/HEVC encoder initialized: "
                   << codec_.width << "x" << codec_.height
                   << " (stride " << hor_stride_ << "x" << ver_stride_ << ")"
                   << " @ " << codec_.maxFramerate << "fps, target_bps="
                   << configuration_.target_bps;

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));

  ReportInit();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH265EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH265EncoderImpl::Release() {
  if (pkt_buf_) {
    mpp_buffer_put(pkt_buf_);
    pkt_buf_ = nullptr;
  }
  if (frame_buf_) {
    mpp_buffer_put(frame_buf_);
    frame_buf_ = nullptr;
  }
  if (frame_group_) {
    mpp_buffer_group_put(frame_group_);
    frame_group_ = nullptr;
  }
  if (mpp_cfg_) {
    mpp_enc_cfg_deinit(mpp_cfg_);
    mpp_cfg_ = nullptr;
  }
  if (mpp_ctx_) {
    mpp_destroy(mpp_ctx_);
    mpp_ctx_ = nullptr;
    mpp_api_ = nullptr;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH265EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!mpp_ctx_ || !mpp_api_) {
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }

  scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  RTC_CHECK(frame_buffer->type() == VideoFrameBuffer::Type::kI420);

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }

  bool send_key_frame =
      is_keyframe_needed ||
      (frame_types && !frame_types->empty() &&
       (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (send_key_frame) {
    is_keyframe_needed = true;
    configuration_.key_frame_request = false;
  }

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (frame_types != nullptr && !frame_types->empty()) {
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  // Request IDR frame if needed
  if (is_keyframe_needed) {
    mpp_api_->control(mpp_ctx_, MPP_ENC_SET_IDR_FRAME, nullptr);
  }

  current_encoding_is_keyframe_ = is_keyframe_needed;

  // Copy I420 data into MPP frame buffer with proper stride alignment
  void* buf = mpp_buffer_get_ptr(frame_buf_);
  uint8_t* dst_y = static_cast<uint8_t*>(buf);
  uint8_t* dst_u = dst_y + hor_stride_ * ver_stride_;
  uint8_t* dst_v = dst_u + (hor_stride_ / 2) * (ver_stride_ / 2);

  libyuv::I420Copy(
      frame_buffer->DataY(), frame_buffer->StrideY(),
      frame_buffer->DataU(), frame_buffer->StrideU(),
      frame_buffer->DataV(), frame_buffer->StrideV(),
      dst_y, hor_stride_,
      dst_u, hor_stride_ / 2,
      dst_v, hor_stride_ / 2,
      codec_.width, codec_.height);

  // Set up MPP frame
  MppFrame frame = nullptr;
  MPP_RET ret = mpp_frame_init(&frame);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "mpp_frame_init failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  mpp_frame_set_width(frame, codec_.width);
  mpp_frame_set_height(frame, codec_.height);
  mpp_frame_set_hor_stride(frame, hor_stride_);
  mpp_frame_set_ver_stride(frame, ver_stride_);
  mpp_frame_set_fmt(frame, MPP_FMT_YUV420P);
  mpp_frame_set_buffer(frame, frame_buf_);
  mpp_frame_set_eos(frame, 0);

  // Set up output packet
  MppPacket packet = nullptr;
  mpp_packet_init_with_buffer(&packet, pkt_buf_);
  mpp_packet_set_length(packet, 0);

  MppMeta meta = mpp_frame_get_meta(frame);
  mpp_meta_set_packet(meta, KEY_OUTPUT_PACKET, packet);

  // Encode
  ret = mpp_api_->encode_put_frame(mpp_ctx_, frame);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "encode_put_frame failed: " << ret;
    mpp_frame_deinit(&frame);
    mpp_packet_deinit(&packet);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  MppPacket out_packet = nullptr;
  ret = mpp_api_->encode_get_packet(mpp_ctx_, &out_packet);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "encode_get_packet failed: " << ret;
    mpp_frame_deinit(&frame);
    mpp_packet_deinit(&packet);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  int32_t result = WEBRTC_VIDEO_CODEC_OK;
  if (out_packet) {
    result = ProcessEncodedPacket(out_packet, input_frame);
    mpp_packet_deinit(&out_packet);
  }

  current_encoding_is_keyframe_ = false;
  mpp_frame_deinit(&frame);
  mpp_packet_deinit(&packet);

  return result;
}

int32_t MppH265EncoderImpl::ProcessEncodedPacket(
    MppPacket packet,
    const VideoFrame& input_frame) {
  void* ptr = mpp_packet_get_pos(packet);
  size_t len = mpp_packet_get_length(packet);

  if (!ptr || len == 0) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.SetRtpTimestamp(input_frame.rtp_timestamp());
  encoded_image_.SetSimulcastIndex(0);
  encoded_image_.ntp_time_ms_ = input_frame.ntp_time_ms();
  encoded_image_.capture_time_ms_ = input_frame.render_time_ms();
  encoded_image_.rotation_ = input_frame.rotation();
  encoded_image_.content_type_ = VideoContentType::UNSPECIFIED;
  encoded_image_.timing_.flags = VideoSendTiming::kInvalid;
  encoded_image_._frameType =
      current_encoding_is_keyframe_ ? VideoFrameType::kVideoFrameKey
                                    : VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(input_frame.color_space());

  auto data = static_cast<const uint8_t*>(ptr);
  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(data, len));
  encoded_image_.set_size(len);

  encoded_image_.qp_ = -1;

  CodecSpecificInfo codec_info;
  codec_info.codecType = kVideoCodecH265;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_info);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "OnEncodedImage callback failed: " << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo MppH265EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "Rockchip MPP H265 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void MppH265EncoderImpl::SetRates(const RateControlParameters& parameters) {
  if (!mpp_ctx_ || !mpp_api_) {
    RTC_LOG(LS_WARNING) << "SetRates() while uninitialized.";
    return;
  }

  if (parameters.framerate_fps < 1.0) {
    RTC_LOG(LS_WARNING) << "Invalid frame rate: " << parameters.framerate_fps;
    return;
  }

  if (parameters.bitrate.get_sum_bps() == 0) {
    configuration_.SetStreamState(false);
    return;
  }

  uint32_t new_target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  float new_fps = parameters.framerate_fps;

  codec_.maxFramerate = static_cast<uint32_t>(new_fps);
  codec_.maxBitrate = new_target_bps;

  configuration_.target_bps = new_target_bps;
  configuration_.max_frame_rate = new_fps;

  // Dynamically update MPP rate control
  if (mpp_cfg_) {
    int fps_num = static_cast<int>(new_fps);
    if (fps_num < 1) fps_num = 30;

    mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_target", new_target_bps);
    mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_max", new_target_bps * 3 / 2);
    mpp_enc_cfg_set_s32(mpp_cfg_, "rc:bps_min", new_target_bps / 2);
    mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_out_num", fps_num);
    mpp_enc_cfg_set_s32(mpp_cfg_, "rc:fps_out_denorm", 1);

    MPP_RET ret = mpp_api_->control(mpp_ctx_, MPP_ENC_SET_CFG, mpp_cfg_);
    if (ret != MPP_OK) {
      RTC_LOG(LS_WARNING) << "Failed to update MPP rate control: " << ret;
    }
  }

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
  } else {
    configuration_.SetStreamState(false);
  }
}

void MppH265EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
