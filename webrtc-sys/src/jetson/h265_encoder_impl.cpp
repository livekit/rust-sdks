#include "h265_encoder_impl.h"

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <string>

#include "absl/strings/match.h"
#include "absl/types/optional.h"
#include "api/video/video_codec_constants.h"
#include "api/video_codecs/scalability_mode.h"
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
#include "jetson_nvmm_buffer.h"

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
  const bool debug = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  (void)settings;
  if (debug) {
    std::fprintf(stderr,
                 "[H265Impl] InitEncode() called: inst=%p, codecType=%d\n",
                 static_cast<const void*>(inst),
                 inst ? static_cast<int>(inst->codecType) : -1);
    std::fflush(stderr);
  }
  if (!inst || inst->codecType != kVideoCodecH265) {
    if (debug) {
      std::fprintf(stderr, "[H265Impl] InitEncode() ERROR: invalid codec type\n");
      std::fflush(stderr);
    }
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->maxFramerate == 0) {
    if (debug) {
      std::fprintf(stderr, "[H265Impl] InitEncode() ERROR: maxFramerate=0\n");
      std::fflush(stderr);
    }
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }
  if (inst->width < 1 || inst->height < 1) {
    if (debug) {
      std::fprintf(stderr,
                   "[H265Impl] InitEncode() ERROR: invalid dimensions %dx%d\n",
                   inst->width, inst->height);
      std::fflush(stderr);
    }
    ReportError();
    return WEBRTC_VIDEO_CODEC_ERR_PARAMETER;
  }

  if (debug) {
    std::fprintf(stderr,
                 "[H265Impl] InitEncode(): %dx%d @ %d fps, startBitrate=%d "
                 "kbps, maxBitrate=%d kbps, simulcast_streams=%zu\n",
                 inst->width, inst->height, inst->maxFramerate,
                 inst->startBitrate, inst->maxBitrate,
                 inst->numberOfSimulcastStreams);
    std::fflush(stderr);
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
    if (debug) {
      std::fprintf(stderr,
                   "[H265Impl] Calling encoder_.Initialize(%d, %d, %d, %d, %d)\n",
                   codec_.width, codec_.height, codec_.maxFramerate,
                   codec_.startBitrate * 1000, key_frame_interval);
      std::fflush(stderr);
    }
    if (!encoder_.Initialize(codec_.width, codec_.height, codec_.maxFramerate,
                             codec_.startBitrate * 1000, key_frame_interval)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize Jetson MMAPI encoder.";
      if (debug) {
        std::fprintf(stderr,
                     "[H265Impl] InitEncode() ERROR: encoder_.Initialize() failed\n");
        std::fflush(stderr);
      }
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
    if (debug) {
      std::fprintf(stderr, "[H265Impl] encoder_.Initialize() succeeded\n");
      std::fflush(stderr);
    }
  }

  ReportInit();

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  if (debug) {
    std::fprintf(stderr,
                 "[H265Impl] InitEncode() completed successfully: %dx%d @ %d "
                 "fps, bitrate=%d bps\n",
                 codec_.width, codec_.height, codec_.maxFramerate,
                 configuration_.target_bps);
    std::fflush(stderr);
  }
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
  static std::atomic<uint64_t> encode_call_count(0);
  static std::atomic<uint64_t> encode_success_count(0);
  static std::atomic<uint64_t> encode_fail_count(0);
  static std::atomic<uint64_t> empty_packet_count(0);
  const bool debug = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  const uint64_t frame_num = encode_call_count.fetch_add(1);
  if (!encoder_.IsInitialized()) {
    if (debug || frame_num < 5) {
      std::fprintf(stderr,
                   "[H265Impl] Encode() called but encoder not initialized "
                   "(frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    ReportError();
    return WEBRTC_VIDEO_CODEC_UNINITIALIZED;
  }
  if (!encoded_image_callback_) {
    RTC_LOG(LS_WARNING)
        << "InitEncode() has been called, but a callback function "
           "has not been set with RegisterEncodeCompleteCallback()";
    if (debug) {
      std::fprintf(stderr,
                   "[H265Impl] No encoded_image_callback_ set (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
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
      if (debug) {
        std::fprintf(stderr,
                     "[H265Impl] Empty frame type requested (frame %lu)\n",
                     frame_num);
        std::fflush(stderr);
      }
      return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
    }
  }

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  auto frame_buffer_base = input_frame.video_frame_buffer();
  if (debug && frame_buffer_base && (frame_num < 5 || frame_num % 100 == 0)) {
    std::fprintf(stderr,
                 "[H265Impl] Encode() input buffer (frame %lu): type=%d "
                 "size=%dx%d keyframe_needed=%d\n",
                 frame_num, static_cast<int>(frame_buffer_base->type()),
                 frame_buffer_base->width(), frame_buffer_base->height(),
                 is_keyframe_needed ? 1 : 0);
    std::fflush(stderr);
  }
  if (frame_buffer_base) {
    if (const auto* nvmm_buffer =
            dynamic_cast<const livekit::JetsonNvmmBuffer*>(
                frame_buffer_base.get())) {
      if (debug || frame_num < 5) {
        std::fprintf(stderr,
                     "[H265Impl] JetsonNvmmBuffer detected via dynamic_cast "
                     "(frame %lu, type=%d, fd=%d)\n",
                     frame_num, static_cast<int>(frame_buffer_base->type()),
                     nvmm_buffer->dmabuf_fd());
        std::fflush(stderr);
      }
      const int32_t encode_result =
          EncodeNvmmBuffer(*nvmm_buffer, input_frame, is_keyframe_needed);
      if (debug || frame_num < 5 || encode_result != WEBRTC_VIDEO_CODEC_OK) {
        std::fprintf(stderr,
                     "[H265Impl] EncodeNvmmBuffer() returned %d (frame %lu)\n",
                     encode_result, frame_num);
        std::fflush(stderr);
      }
      return encode_result;
    }
  }
  if (frame_buffer_base &&
      frame_buffer_base->type() == VideoFrameBuffer::Type::kNative) {
    RTC_LOG(LS_ERROR)
        << "Received native video frame buffer, but it is not a JetsonNvmmBuffer. "
           "Zero-copy is required; refusing I420 fallback.";
    if (debug || frame_num < 10) {
      std::fprintf(stderr,
                   "[H265Impl] Native buffer rejected: type=kNative but "
                   "JetsonNvmmBuffer cast failed (frame %lu)\n",
                   frame_num);
      std::fflush(stderr);
    }
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert frame to I420.";
    if (debug || frame_num < 10) {
      std::fprintf(stderr,
                   "[H265Impl] ToI420() failed (frame %lu, type=%d)\n",
                   frame_num,
                   static_cast<int>(input_frame.video_frame_buffer()->type()));
      std::fflush(stderr);
    }
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }

  RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
  RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

  std::vector<uint8_t> packet;
  bool is_keyframe = false;
  if (!encoder_.Encode(frame_buffer->DataY(), frame_buffer->StrideY(),
                       frame_buffer->DataU(), frame_buffer->StrideU(),
                       frame_buffer->DataV(), frame_buffer->StrideV(),
                       is_keyframe_needed, &packet, &is_keyframe)) {
    encode_fail_count.fetch_add(1);
    RTC_LOG(LS_ERROR) << "Failed to encode frame with Jetson MMAPI encoder.";
    if (debug || frame_num < 10) {
      std::fprintf(stderr,
                   "[H265Impl] encoder_.Encode() failed (frame %lu, "
                   "total_fail=%lu)\n",
                   frame_num, encode_fail_count.load());
      std::fflush(stderr);
    }
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  if (packet.empty()) {
    empty_packet_count.fetch_add(1);
    RTC_LOG(LS_WARNING) << "Jetson MMAPI encoder returned empty packet; "
                           "skipping output.";
    if (debug || frame_num < 10) {
      std::fprintf(stderr,
                   "[H265Impl] Empty packet returned (frame %lu, total_empty=%lu)\n",
                   frame_num, empty_packet_count.load());
      std::fflush(stderr);
    }
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (is_keyframe_needed) {
    configuration_.key_frame_request = false;
  }

  encode_success_count.fetch_add(1);
  if (debug && (frame_num < 5 || frame_num % 100 == 0)) {
    std::fprintf(stderr,
                 "[H265Impl] Encode() success (frame %lu, size=%zu, "
                 "keyframe=%d, success=%lu, fail=%lu, empty=%lu)\n",
                 frame_num, packet.size(), is_keyframe ? 1 : 0,
                 encode_success_count.load(), encode_fail_count.load(),
                 empty_packet_count.load());
    std::fflush(stderr);
  }
  return ProcessEncodedFrame(packet, input_frame, is_keyframe);
}

int32_t JetsonH265EncoderImpl::EncodeNvmmBuffer(
    const livekit::JetsonNvmmBuffer& buffer,
    const ::webrtc::VideoFrame& input_frame,
    bool is_keyframe_needed) {
  RTC_DCHECK_EQ(configuration_.width, buffer.width());
  RTC_DCHECK_EQ(configuration_.height, buffer.height());

  std::vector<uint8_t> packet;
  bool is_keyframe = false;
  if (!encoder_.EncodeNvmmDmabuf(buffer.dmabuf_fd(), is_keyframe_needed,
                                 &packet, &is_keyframe)) {
    RTC_LOG(LS_ERROR)
        << "Failed to encode Jetson NVMM frame with MMAPI encoder. "
           "Zero-copy is required; refusing I420 fallback.";
    return WEBRTC_VIDEO_CODEC_ERROR;
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
  const bool debug = std::getenv("LK_ENCODER_DEBUG") != nullptr;
  if (debug) {
    std::fprintf(stderr,
                 "[H265Impl] OnEncodedImage: size=%zu frameType=%d qp=%d "
                 "result=%d\n",
                 packet.size(), static_cast<int>(encoded_image_._frameType),
                 encoded_image_.qp_, static_cast<int>(result.error));
    std::fflush(stderr);
  }
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
