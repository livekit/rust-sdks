#include "h264_encoder_impl.h"

#include <algorithm>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <limits>
#include <string>

#include "absl/strings/match.h"
#include "absl/types/optional.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "common_video/h264/h264_common.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/svc/create_scalability_structure.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "modules/video_coding/utility/simulcast_utility.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "rtc_base/time_utils.h"
#include "system_wrappers/include/metrics.h"
#include "third_party/libyuv/include/libyuv/convert.h"

namespace webrtc {

// Used by histograms. Values of entries should not be changed.
enum H264EncoderImplEvent {
  kH264EncoderEventInit = 0,
  kH264EncoderEventError = 1,
  kH264EncoderEventMax = 16,
};

JetsonH264EncoderImpl::JetsonH264EncoderImpl(const webrtc::Environment& env,
                                             const SdpVideoFormat& format)
    : env_(env),
      encoder_(livekit::JetsonCodec::kH264),
      packetization_mode_(
          H264EncoderSettings::Parse(format).packetization_mode),
      format_(format) {
  auto it = format_.parameters.find("profile-level-id");
  if (it != format_.parameters.end()) {
    std::optional<webrtc::H264ProfileLevelId> profile_level_id =
        webrtc::ParseH264ProfileLevelId(it->second.c_str());
    if (profile_level_id.has_value()) {
      profile_ = profile_level_id->profile;
      level_ = profile_level_id->level;
    }
  }
}

JetsonH264EncoderImpl::~JetsonH264EncoderImpl() {
  Release();
}

void JetsonH264EncoderImpl::ReportInit() {
  if (has_reported_init_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H264EncoderImpl.Event",
                            kH264EncoderEventInit, kH264EncoderEventMax);
  has_reported_init_ = true;
}

void JetsonH264EncoderImpl::ReportError() {
  if (has_reported_error_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H264EncoderImpl.Event",
                            kH264EncoderEventError, kH264EncoderEventMax);
  has_reported_error_ = true;
}

int32_t JetsonH264EncoderImpl::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  (void)settings;
  if (!inst || inst->codecType != kVideoCodecH264) {
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

  const size_t new_capacity =
      CalcBufferSize(VideoType::kI420, codec_.width, codec_.height);
  encoded_image_.SetEncodedData(EncodedImageBuffer::Create(new_capacity));
  encoded_image_._encodedWidth = codec_.width;
  encoded_image_._encodedHeight = codec_.height;
  encoded_image_.set_size(0);

  configuration_.sending = false;
  configuration_.frame_dropping_on = codec_.GetFrameDropEnabled();
  configuration_.key_frame_interval = codec_.H264()->keyFrameInterval;

  configuration_.width = codec_.width;
  configuration_.height = codec_.height;

  configuration_.max_frame_rate = codec_.maxFramerate;
  configuration_.target_bps = codec_.startBitrate * 1000;
  configuration_.max_bps = codec_.maxBitrate * 1000;

  if (!encoder_.IsInitialized()) {
    int key_frame_interval = codec_.H264()->keyFrameInterval;
    if (key_frame_interval <= 0) {
      key_frame_interval = codec_.maxFramerate * 5;
    }
    if (!encoder_.Initialize(codec_.width, codec_.height, codec_.maxFramerate,
                             codec_.startBitrate * 1000, key_frame_interval)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize Jetson MMAPI encoder.";
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  ReportInit();

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Release() {
  if (encoder_.IsInitialized()) {
    encoder_.Destroy();
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH264EncoderImpl::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_.IsInitialized()) {
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

  bool is_keyframe_needed = configuration_.key_frame_request;
  if (frame_types && !frame_types->empty()) {
    if ((*frame_types)[0] == VideoFrameType::kVideoFrameKey) {
      is_keyframe_needed = true;
    }
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  size_t nv12_size = codec_.width * codec_.height * 3 / 2;
  if (nv12_buffer_.size() != nv12_size) {
    nv12_buffer_.resize(nv12_size);
  }
  uint8_t* dst_y = nv12_buffer_.data();
  uint8_t* dst_uv = nv12_buffer_.data() + codec_.width * codec_.height;
  libyuv::I420ToNV12(frame_buffer->DataY(), frame_buffer->StrideY(),
                     frame_buffer->DataU(), frame_buffer->StrideU(),
                     frame_buffer->DataV(), frame_buffer->StrideV(), dst_y,
                     codec_.width, dst_uv, codec_.width, codec_.width,
                     codec_.height);

  std::vector<uint8_t> packet;
  bool is_keyframe = false;
  if (!encoder_.Encode(dst_y, codec_.width, dst_uv, codec_.width,
                       is_keyframe_needed, &packet, &is_keyframe)) {
    RTC_LOG(LS_ERROR) << "Failed to encode frame with Jetson MMAPI encoder.";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  if (is_keyframe_needed) {
    configuration_.key_frame_request = false;
  }

  return ProcessEncodedFrame(packet, input_frame, is_keyframe);
}

int32_t JetsonH264EncoderImpl::ProcessEncodedFrame(
    std::vector<uint8_t>& packet,
    const ::webrtc::VideoFrame& input_frame,
    bool is_keyframe) {
  static std::atomic<bool> dumped(false);
  static std::atomic<bool> logged_env(false);
  if (!dumped.load(std::memory_order_relaxed)) {
    const char* dump_path = std::getenv("LK_DUMP_H264");
    if (!dump_path || dump_path[0] == '\0') {
      if (!logged_env.exchange(true)) {
        RTC_LOG(LS_INFO)
            << "LK_DUMP_H264 not set; skipping H264 dump.";
      }
    } else if (packet.empty()) {
      if (!logged_env.exchange(true)) {
        RTC_LOG(LS_WARNING)
            << "LK_DUMP_H264 set to " << dump_path
            << " but encoded packet is empty.";
      }
    } else {
      std::error_code ec;
      std::filesystem::path path(dump_path);
      if (path.has_parent_path()) {
        std::filesystem::create_directories(path.parent_path(), ec);
      }
      std::ofstream out(dump_path, std::ios::binary);
      if (out.good()) {
        out.write(reinterpret_cast<const char*>(packet.data()),
                  static_cast<std::streamsize>(packet.size()));
        RTC_LOG(LS_INFO) << "Dumped H264 access unit to " << dump_path
                         << " (bytes=" << packet.size()
                         << ", keyframe=" << is_keyframe << ")";
        dumped.store(true, std::memory_order_relaxed);
      } else {
        RTC_LOG(LS_WARNING) << "Failed to open LK_DUMP_H264 path: "
                            << dump_path;
      }
      logged_env.store(true, std::memory_order_relaxed);
    }
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
      is_keyframe ? VideoFrameType::kVideoFrameKey
                  : VideoFrameType::kVideoFrameDelta;
  encoded_image_.SetColorSpace(input_frame.color_space());

  std::vector<H264::NaluIndex> nalu_indices =
      H264::FindNaluIndices(MakeArrayView(packet.data(), packet.size()));
  for (uint32_t i = 0; i < nalu_indices.size(); i++) {
    const H264::NaluType nalu_type =
        H264::ParseNaluType(packet[nalu_indices[i].payload_start_offset]);
    if (nalu_type == H264::kIdr) {
      encoded_image_._frameType = VideoFrameType::kVideoFrameKey;
      break;
    }
  }

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(packet.data(), packet.size()));
  encoded_image_.set_size(packet.size());

  h264_bitstream_parser_.ParseBitstream(encoded_image_);
  encoded_image_.qp_ = h264_bitstream_parser_.GetLastSliceQp().value_or(-1);

  CodecSpecificInfo codecInfo;
  codecInfo.codecType = kVideoCodecH264;
  codecInfo.codecSpecific.H264.packetization_mode =
      H264PacketizationMode::NonInterleaved;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codecInfo);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode m_encodedCompleteCallback failed "
                      << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo JetsonH264EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "Jetson MMAPI H264 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void JetsonH264EncoderImpl::SetRates(
    const RateControlParameters& parameters) {
  if (!encoder_.IsInitialized()) {
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

  codec_.maxFramerate = static_cast<uint32_t>(parameters.framerate_fps);
  codec_.maxBitrate = parameters.bitrate.GetSpatialLayerSum(0);

  configuration_.target_bps = parameters.bitrate.GetSpatialLayerSum(0);
  configuration_.max_frame_rate = parameters.framerate_fps;

  encoder_.SetRates(codec_.maxFramerate,
                    static_cast<int>(configuration_.target_bps));

  if (configuration_.target_bps) {
    configuration_.SetStreamState(true);
  } else {
    configuration_.SetStreamState(false);
  }
}

void JetsonH264EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
