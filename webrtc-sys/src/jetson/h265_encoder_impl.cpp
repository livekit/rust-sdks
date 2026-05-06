#include "h265_encoder_impl.h"

#include <algorithm>
#include <limits>
#include <string>

#include "absl/strings/match.h"
#include "absl/types/optional.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "livekit/dmabuf_video_frame_buffer.h"
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
enum H265EncoderImplEvent {
  kH265EncoderEventInit = 0,
  kH265EncoderEventError = 1,
  kH265EncoderEventMax = 16,
};

JetsonH265EncoderImpl::JetsonH265EncoderImpl(const webrtc::Environment& env,
                                             const SdpVideoFormat& format)
    : env_(env), encoder_(livekit::JetsonCodec::kH265), format_(format) {}

JetsonH265EncoderImpl::~JetsonH265EncoderImpl() {
  Release();
}

void JetsonH265EncoderImpl::ReportInit() {
  if (has_reported_init_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventInit, kH265EncoderEventMax);
  has_reported_init_ = true;
}

void JetsonH265EncoderImpl::ReportError() {
  if (has_reported_error_) {
    return;
  }
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventError, kH265EncoderEventMax);
  has_reported_error_ = true;
}

int32_t JetsonH265EncoderImpl::InitEncode(
    const VideoCodec* inst,
    const VideoEncoder::Settings& settings) {
  (void)settings;
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

  if (!encoder_.IsInitialized()) {
    int key_frame_interval = codec_.maxFramerate * 5;
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

int32_t JetsonH265EncoderImpl::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH265EncoderImpl::Release() {
  if (encoder_.IsInitialized()) {
    encoder_.Destroy();
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t JetsonH265EncoderImpl::Encode(
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

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }
  if (frame_types && !frame_types->empty()) {
    if ((*frame_types)[0] == VideoFrameType::kVideoFrameKey) {
      is_keyframe_needed = true;
    }
    if ((*frame_types)[0] == VideoFrameType::kEmptyFrame) {
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  std::vector<uint8_t> packet;
  bool is_keyframe = false;

  // Check for DmaBuf zero-copy path first.
  auto* dmabuf = livekit::DmaBufVideoFrameBuffer::FromNative(
      input_frame.video_frame_buffer().get());
  if (dmabuf) {
    if (!encoder_.EncodeDmaBuf(dmabuf->dmabuf_fd(), is_keyframe_needed,
                               &packet, &is_keyframe)) {
      RTC_LOG(LS_ERROR) << "Failed to encode DmaBuf frame with Jetson MMAPI.";
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  } else {
    // Standard I420 path.
    webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
        input_frame.video_frame_buffer()->ToI420();
    if (!frame_buffer) {
      RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
      return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
    }

    RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
    RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

    if (!encoder_.Encode(frame_buffer->DataY(), frame_buffer->StrideY(),
                         frame_buffer->DataU(), frame_buffer->StrideU(),
                         frame_buffer->DataV(), frame_buffer->StrideV(),
                         is_keyframe_needed, &packet, &is_keyframe)) {
      RTC_LOG(LS_ERROR) << "Failed to encode frame with Jetson MMAPI encoder.";
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  if (packet.empty()) {
    RTC_LOG(LS_WARNING) << "Jetson MMAPI encoder returned empty packet; "
                           "skipping output.";
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (is_keyframe_needed) {
    configuration_.key_frame_request = false;
  }

  return ProcessEncodedFrame(packet, input_frame, is_keyframe);
}

int32_t JetsonH265EncoderImpl::ProcessEncodedFrame(
    std::vector<uint8_t>& packet,
    const ::webrtc::VideoFrame& input_frame,
    bool is_keyframe) {
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

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(packet.data(), packet.size()));
  encoded_image_.set_size(packet.size());

  encoded_image_.qp_ = -1;

  CodecSpecificInfo codecInfo;
  codecInfo.codecType = kVideoCodecH265;

  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codecInfo);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode m_encodedCompleteCallback failed "
                      << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo JetsonH265EncoderImpl::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = true;
  info.implementation_name = "Jetson MMAPI H265 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kNative,
                                  VideoFrameBuffer::Type::kI420};
  return info;
}

void JetsonH265EncoderImpl::SetRates(
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

void JetsonH265EncoderImpl::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
