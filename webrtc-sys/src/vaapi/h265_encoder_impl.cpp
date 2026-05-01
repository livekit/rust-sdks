#include "h265_encoder_impl.h"

#include <string>
#include <vector>

#include "api/video/video_codec_constants.h"
#include "common_video/h265/h265_common.h"
#include "common_video/libyuv/include/webrtc_libyuv.h"
#include "modules/video_coding/include/video_codec_interface.h"
#include "modules/video_coding/include/video_error_codes.h"
#include "modules/video_coding/utility/simulcast_rate_allocator.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "system_wrappers/include/metrics.h"

#define VA_FOURCC_I420 0x30323449

namespace webrtc {
namespace {

struct H265NalSummary {
  bool has_vps = false;
  bool has_sps = false;
  bool has_pps = false;
  bool has_irap = false;
  size_t nalu_count = 0;
  std::vector<uint8_t> parameter_sets;
};

H265NalSummary SummarizeH265Bitstream(const std::vector<uint8_t>& bitstream) {
  H265NalSummary summary;
  for (const H265::NaluIndex& nalu : H265::FindNaluIndices(bitstream)) {
    if (nalu.payload_size < H265::kNaluHeaderSize) {
      continue;
    }
    summary.nalu_count++;
    const H265::NaluType type =
        H265::ParseNaluType(bitstream[nalu.payload_start_offset]);
    switch (type) {
      case H265::NaluType::kVps:
        summary.has_vps = true;
        summary.parameter_sets.insert(
            summary.parameter_sets.end(),
            bitstream.begin() + nalu.start_offset,
            bitstream.begin() + nalu.payload_start_offset + nalu.payload_size);
        break;
      case H265::NaluType::kSps:
        summary.has_sps = true;
        summary.parameter_sets.insert(
            summary.parameter_sets.end(),
            bitstream.begin() + nalu.start_offset,
            bitstream.begin() + nalu.payload_start_offset + nalu.payload_size);
        break;
      case H265::NaluType::kPps:
        summary.has_pps = true;
        summary.parameter_sets.insert(
            summary.parameter_sets.end(),
            bitstream.begin() + nalu.start_offset,
            bitstream.begin() + nalu.payload_start_offset + nalu.payload_size);
        break;
      case H265::NaluType::kBlaWLp:
      case H265::NaluType::kBlaWRadl:
      case H265::NaluType::kBlaNLp:
      case H265::NaluType::kIdrWRadl:
      case H265::NaluType::kIdrNLp:
      case H265::NaluType::kCra:
        summary.has_irap = true;
        break;
      default:
        break;
    }
  }
  return summary;
}

}  // namespace

enum H265EncoderImplEvent {
  kH265EncoderEventInit = 0,
  kH265EncoderEventError = 1,
  kH265EncoderEventMax = 16,
};

VAAPIH265EncoderWrapper::VAAPIH265EncoderWrapper(
    const webrtc::Environment& env,
    const SdpVideoFormat& format)
    : env_(env),
      encoder_(new livekit_ffi::VaapiH265EncoderWrapper()),
      format_(format) {}

VAAPIH265EncoderWrapper::~VAAPIH265EncoderWrapper() {
  Release();
}

void VAAPIH265EncoderWrapper::ReportInit() {
  if (has_reported_init_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventInit, kH265EncoderEventMax);
  has_reported_init_ = true;
}

void VAAPIH265EncoderWrapper::ReportError() {
  if (has_reported_error_)
    return;
  RTC_HISTOGRAM_ENUMERATION("WebRTC.Video.H265EncoderImpl.Event",
                            kH265EncoderEventError, kH265EncoderEventMax);
  has_reported_error_ = true;
}

int32_t VAAPIH265EncoderWrapper::InitEncode(
    const VideoCodec* inst,
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
  cached_h265_parameter_sets_.clear();

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

  if (!encoder_->IsInitialized()) {
    int keyFrameInterval = 60;
    if (codec_.maxFramerate > 0) {
      keyFrameInterval = codec_.maxFramerate * 5;
    }
    if (!encoder_->Initialize(codec_.width, codec_.height,
                              codec_.startBitrate * 1000, keyFrameInterval,
                              keyFrameInterval, 1, codec_.maxFramerate,
                              VAProfileHEVCMain, VA_RC_CBR)) {
      RTC_LOG(LS_ERROR) << "Failed to initialize VAAPI H265 encoder";
      ReportError();
      return WEBRTC_VIDEO_CODEC_ERROR;
    }
  }

  SimulcastRateAllocator init_allocator(env_, codec_);
  VideoBitrateAllocation allocation =
      init_allocator.Allocate(VideoBitrateAllocationParameters(
          DataRate::KilobitsPerSec(codec_.startBitrate), codec_.maxFramerate));
  SetRates(RateControlParameters(allocation, codec_.maxFramerate));
  ReportInit();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH265EncoderWrapper::RegisterEncodeCompleteCallback(
    EncodedImageCallback* callback) {
  encoded_image_callback_ = callback;
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH265EncoderWrapper::Release() {
  if (encoder_->IsInitialized()) {
    encoder_->Destroy();
  }
  cached_h265_parameter_sets_.clear();
  return WEBRTC_VIDEO_CODEC_OK;
}

int32_t VAAPIH265EncoderWrapper::Encode(
    const VideoFrame& input_frame,
    const std::vector<VideoFrameType>* frame_types) {
  if (!encoder_) {
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

  webrtc::scoped_refptr<I420BufferInterface> frame_buffer =
      input_frame.video_frame_buffer()->ToI420();
  if (!frame_buffer) {
    RTC_LOG(LS_ERROR) << "Failed to convert "
                      << VideoFrameBufferTypeToString(
                             input_frame.video_frame_buffer()->type())
                      << " image to I420. Can't encode frame.";
    return WEBRTC_VIDEO_CODEC_ENCODER_FAILURE;
  }
  RTC_CHECK(frame_buffer->type() == VideoFrameBuffer::Type::kI420 ||
            frame_buffer->type() == VideoFrameBuffer::Type::kI420A);

  bool is_keyframe_needed = false;
  if (configuration_.key_frame_request && configuration_.sending) {
    is_keyframe_needed = true;
  }

  bool send_key_frame =
      is_keyframe_needed ||
      (frame_types && (*frame_types)[0] == VideoFrameType::kVideoFrameKey);
  if (send_key_frame) {
    is_keyframe_needed = true;
    configuration_.key_frame_request = false;
  }

  RTC_DCHECK_EQ(configuration_.width, frame_buffer->width());
  RTC_DCHECK_EQ(configuration_.height, frame_buffer->height());

  if (!configuration_.sending) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  if (frame_types != nullptr && (*frame_types)[0] == VideoFrameType::kEmptyFrame) {
    return WEBRTC_VIDEO_CODEC_NO_OUTPUT;
  }

  std::vector<uint8_t> output;
  const bool encode_ok = encoder_->Encode(
      VA_FOURCC_I420, frame_buffer->DataY(), frame_buffer->StrideY(),
      frame_buffer->DataU(), frame_buffer->StrideU(), frame_buffer->DataV(),
      frame_buffer->StrideV(), send_key_frame, output);

  if (!encode_ok || output.empty()) {
    RTC_LOG(LS_ERROR) << "Failed to encode H265 frame.";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  H265NalSummary nal_summary = SummarizeH265Bitstream(output);
  if (nal_summary.nalu_count == 0) {
    RTC_LOG(LS_ERROR) << "Encoded H265 frame has no Annex-B NAL units.";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (send_key_frame && !nal_summary.has_irap) {
    RTC_LOG(LS_ERROR)
        << "VAAPI H265 encoder did not produce an IRAP NAL for a requested "
           "keyframe.";
    return WEBRTC_VIDEO_CODEC_ERROR;
  }
  if (nal_summary.has_vps && nal_summary.has_sps && nal_summary.has_pps) {
    cached_h265_parameter_sets_ = nal_summary.parameter_sets;
  } else if (nal_summary.has_irap) {
    if (!cached_h265_parameter_sets_.empty()) {
      output.insert(output.begin(), cached_h265_parameter_sets_.begin(),
                    cached_h265_parameter_sets_.end());
      nal_summary = SummarizeH265Bitstream(output);
    } else {
      RTC_LOG(LS_WARNING)
          << "VAAPI H265 keyframe is missing VPS/SPS/PPS NALs; remote "
             "decoders may not be able to start rendering.";
    }
  }

  encoded_image_.SetEncodedData(
      EncodedImageBuffer::Create(output.data(), output.size()));
  encoded_image_.set_size(output.size());
  encoded_image_.qp_ = -1;
  encoded_image_._encodedWidth = configuration_.width;
  encoded_image_._encodedHeight = configuration_.height;
  encoded_image_.SetRtpTimestamp(input_frame.rtp_timestamp());
  encoded_image_.SetSimulcastIndex(0);
  encoded_image_.ntp_time_ms_ = input_frame.ntp_time_ms();
  encoded_image_.capture_time_ms_ = input_frame.render_time_ms();
  encoded_image_.rotation_ = input_frame.rotation();
  encoded_image_.content_type_ = VideoContentType::UNSPECIFIED;
  encoded_image_.timing_.flags = VideoSendTiming::kInvalid;
  encoded_image_.SetColorSpace(input_frame.color_space());
  encoded_image_._frameType = nal_summary.has_irap
                                  ? VideoFrameType::kVideoFrameKey
                                  : VideoFrameType::kVideoFrameDelta;

  CodecSpecificInfo codec_specific;
  codec_specific.codecType = kVideoCodecH265;
  const auto result =
      encoded_image_callback_->OnEncodedImage(encoded_image_, &codec_specific);
  if (result.error != EncodedImageCallback::Result::OK) {
    RTC_LOG(LS_ERROR) << "Encode callback failed " << result.error;
    return WEBRTC_VIDEO_CODEC_ERROR;
  }

  return WEBRTC_VIDEO_CODEC_OK;
}

VideoEncoder::EncoderInfo VAAPIH265EncoderWrapper::GetEncoderInfo() const {
  EncoderInfo info;
  info.supports_native_handle = false;
  info.implementation_name = "VAAPI H265 Encoder";
  info.scaling_settings = VideoEncoder::ScalingSettings::kOff;
  info.is_hardware_accelerated = true;
  info.supports_simulcast = false;
  info.preferred_pixel_formats = {VideoFrameBuffer::Type::kI420};
  return info;
}

void VAAPIH265EncoderWrapper::SetRates(
    const RateControlParameters& parameters) {
  if (!encoder_) {
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
  encoder_->UpdateRates(configuration_.max_frame_rate,
                        configuration_.target_bps);

  configuration_.SetStreamState(configuration_.target_bps != 0);
}

void VAAPIH265EncoderWrapper::LayerConfig::SetStreamState(bool send_stream) {
  if (send_stream && !sending) {
    key_frame_request = true;
  }
  sending = send_stream;
}

}  // namespace webrtc
