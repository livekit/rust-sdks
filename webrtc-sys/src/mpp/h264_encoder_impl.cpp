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

#include "h264_encoder_impl.h"

#include <array>
#include <cstdint>
#include <optional>

#include "api/video/video_codec_constants.h"
#include "api/video/nv12_buffer.h"
#include "common_video/h264/h264_common.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "rtc_base/logging.h"
#include "third_party/libyuv/include/libyuv/convert.h"
#include "third_party/libyuv/include/libyuv/planar_functions.h"

// MPP alignment macros
#define MPP_ALIGN(x, a) (((x) + (a) - 1) & ~((a) - 1))

namespace webrtc {

namespace {

struct H264LevelLimit {
  H264Level level;
  int max_macroblocks_per_frame;
  int max_macroblocks_per_second;
};

// H.264 table A-1 limits through the highest level understood by libwebrtc.
constexpr std::array<H264LevelLimit, 15> kH264LevelLimits = {{
    {H264Level::kLevel1, 99, 1485},
    {H264Level::kLevel1_1, 396, 3000},
    {H264Level::kLevel1_2, 396, 6000},
    {H264Level::kLevel1_3, 396, 11880},
    {H264Level::kLevel2, 396, 11880},
    {H264Level::kLevel2_1, 792, 19800},
    {H264Level::kLevel2_2, 1620, 20250},
    {H264Level::kLevel3, 1620, 40500},
    {H264Level::kLevel3_1, 3600, 108000},
    {H264Level::kLevel3_2, 5120, 216000},
    {H264Level::kLevel4, 8192, 245760},
    {H264Level::kLevel4_1, 8192, 245760},
    {H264Level::kLevel4_2, 8704, 522240},
    {H264Level::kLevel5, 22080, 589824},
    {H264Level::kLevel5_1, 36864, 983040},
}};

std::optional<H264Level> RequiredH264Level(int width,
                                           int height,
                                           int frames_per_second) {
  const int64_t macroblocks_wide = (static_cast<int64_t>(width) + 15) / 16;
  const int64_t macroblocks_high = (static_cast<int64_t>(height) + 15) / 16;
  const int64_t macroblocks_per_frame = macroblocks_wide * macroblocks_high;
  const int64_t macroblocks_per_second =
      macroblocks_per_frame * frames_per_second;

  for (const H264LevelLimit& limit : kH264LevelLimits) {
    if (macroblocks_per_frame <= limit.max_macroblocks_per_frame &&
        macroblocks_per_second <= limit.max_macroblocks_per_second) {
      return limit.level;
    }
  }

  // Level 5.2 has the same frame-size limit as 5.1 but permits a higher
  // macroblock rate.
  if (macroblocks_per_frame <= 36864 &&
      macroblocks_per_second <= 2073600) {
    return H264Level::kLevel5_2;
  }
  return std::nullopt;
}

}  // namespace

MppH264EncoderImpl::MppH264EncoderImpl(const webrtc::Environment& env,
                                       const SdpVideoFormat& format)
    : env_(env),
      packetization_mode_(H264EncoderSettings::Parse(format)
                              .packetization_mode) {
  std::optional<H264ProfileLevelId> profile_level_id =
      ParseSdpForH264ProfileLevelId(format.parameters);
  if (profile_level_id.has_value()) {
    profile_ = profile_level_id->profile;
    level_ = profile_level_id->level;
  }
}

MppH264EncoderImpl::~MppH264EncoderImpl() = default;

int32_t MppH264EncoderImpl::InitMpp() {
  MPP_RET ret = session_.Initialize(MPP_VIDEO_CodingAVC);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to initialize MPP H.264 encoder: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH264EncoderImpl::ConfigureMpp() {
  MPP_RET ret = MPP_OK;

  ret = session_.InitializeConfig();
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to initialize MPP H.264 config: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // ---- Prep config (input frame format) ----
  mpp_enc_cfg_set_s32(session_.config(), "prep:width", codec_.width);
  mpp_enc_cfg_set_s32(session_.config(), "prep:height", codec_.height);
  mpp_enc_cfg_set_s32(session_.config(), "prep:hor_stride", hor_stride_);
  mpp_enc_cfg_set_s32(session_.config(), "prep:ver_stride", ver_stride_);
  // Use I420 directly to avoid extra conversion
  mpp_enc_cfg_set_s32(session_.config(), "prep:format", MPP_FMT_YUV420P);
  configured_fmt_ = MPP_FMT_YUV420P;

  // ---- Rate control config ----
  mpp_enc_cfg_set_s32(session_.config(), "rc:mode", MPP_ENC_RC_MODE_CBR);
  mpp_enc_cfg_set_s32(session_.config(), "rc:bps_target", configuration_.target_bps);
  mpp_enc_cfg_set_s32(session_.config(), "rc:bps_max",
                      configuration_.target_bps * 3 / 2);
  mpp_enc_cfg_set_s32(session_.config(), "rc:bps_min", configuration_.target_bps / 2);

  // Frame rate
  int fps_num = static_cast<int>(configuration_.max_frame_rate);
  if (fps_num < 1) {
    fps_num = 30;
  }
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_in_flex", 0);
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_in_num", fps_num);
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_in_denorm", 1);
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_out_flex", 0);
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_out_num", fps_num);
  mpp_enc_cfg_set_s32(session_.config(), "rc:fps_out_denorm", 1);

  // Keep a bounded fallback GOP while still honoring explicit keyframe requests.
  mpp_enc_cfg_set_s32(session_.config(), "rc:gop", fps_num * 10);

  // ---- H.264 codec config ----
  mpp_enc_cfg_set_s32(session_.config(), "codec:id", MPP_VIDEO_CodingAVC);

  // Profile values are H.264 profile_idc values. CABAC is forbidden for
  // Baseline/Constrained Baseline and must follow the negotiated profile.
  int mpp_profile = 66;
  bool enable_cabac = false;
  switch (profile_) {
    case H264Profile::kProfileConstrainedBaseline:
    case H264Profile::kProfileBaseline:
      mpp_profile = 66;
      break;
    case H264Profile::kProfileMain:
      mpp_profile = 77;
      enable_cabac = true;
      break;
    case H264Profile::kProfileConstrainedHigh:
    case H264Profile::kProfileHigh:
    case H264Profile::kProfilePredictiveHigh444:
    default:
      mpp_profile = 100;
      enable_cabac = true;
      break;
  }
  int mpp_level = static_cast<int>(level_);
  if (level_ == H264Level::kLevel1_b) {
    mpp_level = 11;
    constexpr RK_U32 kForceConstraintSet3 = (1U << 19) | (1U << 3);
    mpp_enc_cfg_set_u32(session_.config(), "h264:constraint_set",
                        kForceConstraintSet3);
  }
  mpp_enc_cfg_set_s32(session_.config(), "h264:profile", mpp_profile);
  mpp_enc_cfg_set_s32(session_.config(), "h264:level", mpp_level);
  mpp_enc_cfg_set_s32(session_.config(), "h264:cabac_en", enable_cabac ? 1 : 0);
  mpp_enc_cfg_set_s32(session_.config(), "h264:cabac_idc", 0);
  mpp_enc_cfg_set_s32(session_.config(), "h264:trans8x8", (mpp_profile == 100) ? 1 : 0);

  // QP range for real-time streaming
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_init", 26);
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_max", 48);
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_min", 8);
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_max_i", 48);
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_min_i", 8);
  mpp_enc_cfg_set_s32(session_.config(), "rc:qp_delta_ip", 6);

  ret = session_.api()->control(session_.context(), MPP_ENC_SET_CFG, session_.config());
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "MPP_ENC_SET_CFG failed: " << ret;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  // Set header mode: attach SPS/PPS on each IDR
  MppEncHeaderMode header_mode = MPP_ENC_HEADER_MODE_EACH_IDR;
  ret = session_.api()->control(session_.context(), MPP_ENC_SET_HEADER_MODE, &header_mode);
  if (ret != MPP_OK) {
    RTC_LOG(LS_WARNING) << "MPP_ENC_SET_HEADER_MODE failed: " << ret;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH264EncoderImpl::InitEncode(const VideoCodec* inst,
                                       const VideoEncoder::Settings& settings) {
  (void)settings;
  if (!inst || inst->codecType != kVideoCodecH264) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  std::optional<H264Level> required_level = RequiredH264Level(
      inst->width, inst->height, static_cast<int>(inst->maxFramerate));
  if (!required_level.has_value()) {
    const int64_t macroblocks_per_frame =
        ((static_cast<int64_t>(inst->width) + 15) / 16) *
        ((static_cast<int64_t>(inst->height) + 15) / 16);
    RTC_LOG(LS_ERROR)
        << "MPP H.264 cannot publish " << inst->width << "x" << inst->height
        << " @ " << inst->maxFramerate
        << "fps as a WebRTC-compatible stream: geometry/rate requires a "
           "level above 5.2 ("
        << macroblocks_per_frame
        << " macroblocks/frame; maximum is 36864). Reduce the capture "
           "resolution (for example, 3840x1080) or use H.265.";
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  // MPP otherwise raises the SPS level from frame geometry alone and ignores
  // macroblock rate. Configure the minimum truthful level ourselves so the
  // emitted SPS remains consistent with the advertised WebRTC capability.
  level_ = required_level.value();

  int32_t release_ret = Release();
  if (release_ret != WEBRTC_VIDEO_CODEC_OK) {
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
  // I420: Y plane = hor_stride * ver_stride, U+V = hor_stride * ver_stride / 2
  frame_size_ = static_cast<size_t>(hor_stride_) *
                static_cast<size_t>(ver_stride_) * 3 / 2;

  configuration_.sending = false;
  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;

  // Initialize MPP encoder
  int32_t mpp_ret = InitMpp();
  if (mpp_ret != WEBRTC_VIDEO_CODEC_OK) {
    Release();
    return mpp_ret;
  }

  // Configure MPP encoder
  mpp_ret = ConfigureMpp();
  if (mpp_ret != WEBRTC_VIDEO_CODEC_OK) {
    Release();
    return mpp_ret;
  }

  // A raw-frame-sized packet buffer safely accommodates worst-case access
  // units while keeping allocation bounded by the input frame size.
  MPP_RET ret = session_.AllocateBuffers(frame_size_, frame_size_);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to allocate MPP encoder buffers: " << ret;
    Release();
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  RTC_LOG(LS_INFO) << "Rockchip MPP H264 encoder initialized: "
                   << codec_.width << "x" << codec_.height
                   << " (stride " << hor_stride_ << "x" << ver_stride_ << ")"
                   << " @ " << codec_.maxFramerate << "fps, target_bps="
                   << configuration_.target_bps
                   << ", H.264 level=" << static_cast<int>(level_);

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));

  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH264EncoderImpl::Release() {
  session_.Reset();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t MppH264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!session_.context() || !session_.api()) {
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }
  if (frame_types != nullptr && !frame_types->empty() &&
      (*frame_types)[0] == VideoFrameType::kEmptyFrame) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  webrtc::scoped_refptr<VideoFrameBuffer> vfb = input_frame.video_frame_buffer();
  if (vfb->width() != codec_.width || vfb->height() != codec_.height) {
    RTC_LOG(LS_ERROR) << "MPP H.264 input frame dimensions " << vfb->width()
                      << "x" << vfb->height() << " do not match encoder "
                      << codec_.width << "x" << codec_.height;
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  const bool is_nv12 = (vfb->type() == VideoFrameBuffer::Type::kNV12);

  scoped_refptr<I420BufferInterface> i420_buffer;
  const NV12BufferInterface* nv12_buffer = nullptr;

  if (is_nv12) {
    nv12_buffer = vfb->GetNV12();
    if (!nv12_buffer) {
      RTC_LOG(LS_ERROR) << "NV12 frame did not provide an NV12 buffer.";
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
  } else {
    i420_buffer = vfb->ToI420();
    if (!i420_buffer) {
      RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
  }

  const bool is_keyframe_needed =
      configuration_.key_frame_request ||
      (frame_types && !frame_types->empty() &&
       (*frame_types)[0] == VideoFrameType::kVideoFrameKey);

  // Request IDR frame if needed
  if (is_keyframe_needed) {
    MPP_RET idr_result =
        session_.api()->control(session_.context(), MPP_ENC_SET_IDR_FRAME, nullptr);
    if (idr_result == MPP_OK) {
      configuration_.key_frame_request = false;
    } else {
      RTC_LOG(LS_WARNING) << "MPP H.264 IDR request failed: " << idr_result;
    }
  }

  void* buf = mpp_buffer_get_ptr(session_.frame_buffer());
  if (!buf) {
    RTC_LOG(LS_ERROR) << "MPP H.264 frame buffer is not CPU-accessible";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  MPP_RET sync_result = mpp_buffer_sync_begin(session_.frame_buffer());
  if (sync_result != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to begin MPP H.264 buffer access: "
                      << sync_result;
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  MppFrameFormat mpp_fmt;
  int copy_result;
  const size_t y_plane_size = static_cast<size_t>(hor_stride_) *
                              static_cast<size_t>(ver_stride_);

  if (is_nv12) {
    // NV12 (YUV420SP): Y plane + interleaved UV plane — native MPP format
    uint8_t* dst_y = static_cast<uint8_t*>(buf);
    uint8_t* dst_uv = dst_y + y_plane_size;

    copy_result = libyuv::NV12Copy(
        nv12_buffer->DataY(), nv12_buffer->StrideY(),
        nv12_buffer->DataUV(), nv12_buffer->StrideUV(),
        dst_y, hor_stride_,
        dst_uv, hor_stride_,
        codec_.width, codec_.height);
    mpp_fmt = MPP_FMT_YUV420SP;
  } else {
    // I420 (YUV420P): separate Y, U, V planes
    uint8_t* dst_y = static_cast<uint8_t*>(buf);
    uint8_t* dst_u = dst_y + y_plane_size;
    const size_t chroma_plane_size =
        static_cast<size_t>(hor_stride_ / 2) *
        static_cast<size_t>(ver_stride_ / 2);
    uint8_t* dst_v = dst_u + chroma_plane_size;

    copy_result = libyuv::I420Copy(
        i420_buffer->DataY(), i420_buffer->StrideY(),
        i420_buffer->DataU(), i420_buffer->StrideU(),
        i420_buffer->DataV(), i420_buffer->StrideV(),
        dst_y, hor_stride_,
        dst_u, hor_stride_ / 2,
        dst_v, hor_stride_ / 2,
        codec_.width, codec_.height);
    mpp_fmt = MPP_FMT_YUV420P;
  }

  sync_result = mpp_buffer_sync_end(session_.frame_buffer());
  if (copy_result != 0) {
    RTC_LOG(LS_ERROR) << "Failed to copy input frame: " << copy_result;
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  if (sync_result != MPP_OK) {
    RTC_LOG(LS_ERROR) << "Failed to finish MPP H.264 buffer access: "
                      << sync_result;
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Reconfigure MPP prep:format when the input pixel format changes
  if (mpp_fmt != configured_fmt_) {
    mpp_enc_cfg_set_s32(session_.config(), "prep:format", mpp_fmt);
    MPP_RET cfg_ret = session_.api()->control(
        session_.context(), MPP_ENC_SET_CFG, session_.config());
    if (cfg_ret == MPP_OK) {
      RTC_LOG(LS_INFO) << "MPP H264 prep:format reconfigured from "
                       << configured_fmt_ << " to " << mpp_fmt;
      configured_fmt_ = mpp_fmt;
    } else {
      RTC_LOG(LS_ERROR) << "MPP H264 prep:format reconfigure failed: "
                        << cfg_ret;
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }
  }

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
  mpp_frame_set_fmt(frame, mpp_fmt);
  mpp_frame_set_buffer(frame, session_.frame_buffer());
  mpp_frame_set_eos(frame, 0);

  // Set up output packet
  MppPacket packet = nullptr;
  ret = mpp_packet_init_with_buffer(&packet, session_.packet_buffer());
  if (ret != MPP_OK || !packet) {
    RTC_LOG(LS_ERROR) << "mpp_packet_init_with_buffer failed: " << ret;
    mpp_frame_deinit(&frame);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  mpp_packet_set_length(packet, 0);

  // Attach output packet via metadata
  MppMeta meta = mpp_frame_get_meta(frame);
  if (!meta) {
    RTC_LOG(LS_ERROR) << "mpp_frame_get_meta returned null";
    mpp_frame_deinit(&frame);
    mpp_packet_deinit(&packet);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  ret = mpp_meta_set_packet(meta, KEY_OUTPUT_PACKET, packet);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "mpp_meta_set_packet failed: " << ret;
    mpp_frame_deinit(&frame);
    mpp_packet_deinit(&packet);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  // Encode: put frame and get packet
  ret = session_.api()->encode_put_frame(session_.context(), frame);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "encode_put_frame failed: " << ret;
    mpp_frame_deinit(&frame);
    mpp_packet_deinit(&packet);
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  ret = session_.api()->encode_get_packet(session_.context(), &packet);
  if (ret != MPP_OK) {
    RTC_LOG(LS_ERROR) << "encode_get_packet failed: " << ret;
    mpp_frame_deinit(&frame);
    // The packet may still be held by MPP after a failed dequeue.
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  int32_t result = WEBRTC_VIDEO_CODEC_OK;
  if (packet) {
    result = ProcessEncodedPacket(packet, input_frame);
    mpp_packet_deinit(&packet);
  }

  mpp_frame_deinit(&frame);
  return result;
}

int32_t MppH264EncoderImpl::ProcessEncodedPacket(
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
  encoded_image_._frameType = VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(input_frame.color_space());

  // Parse NALUs to determine frame type
  auto data = static_cast<const uint8_t*>(ptr);
  std::vector<H264::NaluIndex> nalu_indices =
      H264::FindNaluIndices(MakeArrayView(data, len));
  for (const auto& nalu_index : nalu_indices) {
    H264::NaluType nalu_type =
        H264::ParseNaluType(data[nalu_index.payload_start_offset]);
    if (nalu_type == H264::kIdr) {
      encoded_image_._frameType = VideoFrameType::kVideoFrameKey;
      break;
    }
  }

  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(data, len));
  encoded_image_.set_size(len);

  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ = h264_bitstream_parser_.GetLastSliceQp().value_or(-1);

  CodecSpecificInfo codec_info;
  codec_info.codecType = kVideoCodecH264;
  codec_info.codecSpecific.H264.packetization_mode =
      packetization_mode_;
  codec_info.codecSpecific.H264.temporal_idx = kNoTemporalIdx;
  codec_info.codecSpecific.H264.base_layer_sync = false;
  codec_info.codecSpecific.H264.idr_frame =
      encoded_image_._frameType == VideoFrameType::kVideoFrameKey;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_info);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "OnEncodedImage callback failed: " << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo MppH264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "Rockchip MPP H264 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kNV12,
                                  VideoFrameBuffer::Type::kI420};
  return info;
}

void MppH264EncoderImpl::SetRates(const RateControlParameters& parameters) {
  if (!session_.context() || !session_.api()) {
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
  codec_.maxBitrate = new_target_bps / 1000;

  configuration_.target_bps = new_target_bps;
  configuration_.max_frame_rate = new_fps;

  // Dynamically update MPP rate control
  if (session_.config()) {
    int fps_num = static_cast<int>(new_fps);
    if (fps_num < 1) {
      fps_num = 30;
    }

    mpp_enc_cfg_set_s32(session_.config(), "rc:bps_target", new_target_bps);
    mpp_enc_cfg_set_s32(session_.config(), "rc:bps_max", new_target_bps * 3 / 2);
    mpp_enc_cfg_set_s32(session_.config(), "rc:bps_min", new_target_bps / 2);
    mpp_enc_cfg_set_s32(session_.config(), "rc:fps_in_num", fps_num);
    mpp_enc_cfg_set_s32(session_.config(), "rc:fps_in_denorm", 1);
    mpp_enc_cfg_set_s32(session_.config(), "rc:fps_out_num", fps_num);
    mpp_enc_cfg_set_s32(session_.config(), "rc:fps_out_denorm", 1);

    MPP_RET ret = session_.api()->control(
        session_.context(), MPP_ENC_SET_CFG, session_.config());
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

void MppH264EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
